#!/usr/bin/env python3
"""Run metamod-audit in disposable GoldSrc server copies (Python 3.10+)."""

import argparse
from contextlib import nullcontext
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import struct
import subprocess
import tempfile
import threading
import time
import uuid

SCHEMA = 1
REPO = Path(__file__).resolve().parents[2]
PREFIX = b"MM_AUDIT "
STATUSES = {"PASS", "FAIL", "SKIP", "INCONCLUSIVE"}
CLASSIFICATIONS = {
    "Verified portable behavior", "Implementation-specific behavior",
    "Unverified host assumption", "SDK defect", "Host defect",
    "Unsupported configuration",
}
MAX_EVENTS = 4096
LOG_LIMIT = 64 * 1024 * 1024
SOURCE = REPO / "metamod-audit/src/lib.rs"
CONTRACTS = {
    "ffi.pointer_readability.v1": "native inputs, return buffers and hook result slots remain readable for the contracted lifetime",
    "ffi.string_reentry.v1": "native activity invisible to SDK callbacks leaves return buffers valid until copied",
    "ffi.registration_ownership.v1": "host retains or copies registration storage and only frees its own allocations",
    "ffi.edict_arena.v1": "map lifecycle callbacks precede edict-arena retirement and serials distinguish slot allocations",
    "ffi.shutdown_order.v1": "host keeps callback code and active hook globals live until native frames unwind",
}


def dump(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=True) + "\n", encoding="utf-8")


def digest(path):
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(block)
    return result.hexdigest()


def catalog():
    return dict(re.findall(r'id: "([^"]+)",\s*expected: "([^"]+)"', SOURCE.read_text(encoding="utf-8")))


def parse_event(line):
    # Console timestamps or engine logging prefixes may precede a record.
    _, marker, payload = line.partition(PREFIX)
    if not marker or len(payload) > 8192:
        return None
    try:
        event = json.loads(payload)
    except (ValueError, UnicodeError):
        return None
    if not isinstance(event, dict) or event.get("schema_version") != SCHEMA:
        return None
    if type(event.get("seq")) is not int or not 1 <= event["seq"] <= MAX_EVENTS:
        return None
    if event.get("kind") == "result":
        if (not all(isinstance(event.get(k), str) for k in ("test", "result", "expected", "observed"))
                or event["result"] not in STATUSES):
            return None
    elif event.get("kind") not in {"ready", "status", "report", "usage", "suite_complete", "limit"}:
        return None
    return event


def read_events(paths):
    events = {}
    diagnostics = []
    warnings = []
    versions = []
    for path in paths:
        with path.open("rb") as stream:
            while line := stream.readline(16384):
                event = parse_event(line)
                if event:
                    seq = event["seq"]
                    if seq in events and events[seq] != event:
                        warnings.append(f"{path.name}: conflicting record at sequence {seq}")
                        break
                    events[seq] = event
                elif PREFIX in line and len(warnings) < 32:
                    warnings.append(f"{path.name}: invalid or truncated protocol record")
                if re.search(rb"Sanitizer|runtime error:|segmentation fault|access violation|Host_Error", line, re.I):
                    if len(diagnostics) < 128:
                        diagnostics.append(line[:2048].decode("utf-8", "replace").strip())
                if re.search(rb"Protocol version|Exe version|Exe build|ReHLDS version|Metamod.*version", line, re.I):
                    if len(versions) < 32:
                        versions.append(line[:1024].decode("utf-8", "replace").strip())
    if events and len(events) != max(events):
        warnings.append("one or more event sequence numbers are missing")
    return [events[i] for i in sorted(events)], diagnostics, warnings, versions


def findings(events, executed, manual, unsupported=None):
    tests = catalog()
    results = {}
    history = {}
    for event in events:
        if event["kind"] != "result":
            continue
        test = event["test"]
        history.setdefault(test, []).append(event["seq"])
        if results.get(test, {}).get("result") != "FAIL":
            results[test] = {**event, "source": "plugin"}
    for item in manual:
        test = item["test"]
        if test in CONTRACTS:
            continue
        if results.get(test, {}).get("result") in {"PASS", "FAIL"}:
            if item["result"] == results[test]["result"]:
                results[test]["manual_evidence"] = item["observed"]
            elif item["result"] in {"PASS", "FAIL"}:
                results[test]["manual_evidence"] = item["observed"]
                results[test]["manual_conflict"] = item["result"]
            continue
        results[test] = {**item, "expected": tests[test], "source": "manual"}
    for test, expected in tests.items():
        results.setdefault(test, {
            "test": test, "expected": expected, "result": "SKIP", "source": "runner",
            "observed": unsupported or ("no observation recorded" if executed else "runtime did not execute"),
            "classification": "Unsupported configuration" if unsupported else "Unverified host assumption",
        })
    for test, result in results.items():
        result["event_sequences"] = history.get(test, [])
        result.setdefault("classification", "Unverified host assumption")
        if result["result"] in {"PASS", "FAIL"} and result["source"] == "plugin":
            result["classification"] = "Implementation-specific behavior"
    return sorted(results.values(), key=lambda item: item["test"])


def manual_results(path):
    if path is None:
        return []
    records = json.loads(path.read_text(encoding="utf-8"))
    seen = set()
    for item in records:
        if (item["test"] not in catalog() | CONTRACTS or item["test"] in seen or item["result"] not in STATUSES
                or item["classification"] not in CLASSIFICATIONS or not item["observed"].strip()
                or not item["operator"].strip()):
            raise ValueError("invalid manual finding (require unique known test, result, classification, evidence and operator)")
        seen.add(item["test"])
    return records


def aggregate(output, runtime, metadata, logs, manual=()):
    events, diagnostics, warnings, versions = read_events(logs)
    if any(event["kind"] == "limit" for event in events):
        warnings.append("plugin event budget reached; later observations may be missing")
    with (output / "events.jsonl").open("w", encoding="utf-8") as stream:
        for event in events:
            record = {**event, "run_id": metadata["run_id"], "runtime_id": runtime["id"]}
            stream.write(json.dumps(record) + "\n")
    results = findings(events, metadata["executed"], manual, runtime.get("unsupported_reason"))
    contract_evidence = {item["test"]: item for item in manual if item["test"] in CONTRACTS}
    status_seq = next((event["seq"] for event in events if event["kind"] == "status"), MAX_EVENTS)
    suite_seq = next((event["seq"] for event in events if event["seq"] > status_seq
                      and event["kind"] == "suite_complete" and event.get("suite") == "all"), MAX_EVENTS)
    protocol_complete = any(event["seq"] > suite_seq and event["kind"] == "report" for event in events)
    return {
        "schema_version": SCHEMA, "runtime": runtime, **metadata,
        "protocol_complete": protocol_complete,
        "console_version_observations": versions,
        "diagnostics": diagnostics, "protocol_warnings": warnings,
        "event_log": str(output / "events.jsonl"), "tests": results,
        "contract_findings": [{"id": contract, "classification": "Unverified host assumption",
                               "expected": expected, "observed": "requires evidence beyond public API behavior",
                               **contract_evidence.get(contract, {})}
                              for contract, expected in CONTRACTS.items()],
    }


def markdown(report):
    lines = ["# GoldSrc runtime audit", "", f"Run `{report['run_id']}`", ""]
    for run in report["runs"]:
        runtime = run["runtime"]
        lines.extend([f"## {runtime['id']}", "",
                      f"Execution **{run['execution']}**, exit status `{run['exit_status']}`, "
                      f"console protocol {'complete' if run['protocol_complete'] else 'incomplete'}", ""])
        for component, identity in runtime.get("components", {}).items():
            lines.append(f"- {component} — {identity['implementation']} {identity['version']} (`{identity.get('sha256', 'unavailable')}`)")
        if plugin := runtime.get("plugin"):
            lines.append(f"- Plugin SHA-256 `{plugin['sha256']}`, audit commit `{plugin.get('audit_plugin_commit')}`, SDK commit `{plugin.get('sdk_commit')}`")
        if run.get("error"):
            lines.extend(["", str(run["error"])])
        lines.extend(["", "| Test | Result | Classification | Evidence |", "| --- | --- | --- | --- |"])
        for test in run["tests"]:
            evidence = test["observed"].replace("|", r"\|").replace("\n", " ")
            lines.append(f"| `{test['test']}` | {test['result']} | {test['classification']} | {evidence} |")
        if run.get("diagnostics"):
            lines.extend(["", "Diagnostics are recorded in the JSON report and raw console logs."])
        if run["protocol_warnings"]:
            lines.extend(["", *[f"- {warning}" for warning in run["protocol_warnings"]]])
        lines.extend(["", "Native ownership and lifetime assumptions remain recorded separately in the JSON report."])
        lines.append("")
    return "\n".join(lines)


def finish(output, run_id, runs):
    result = {"schema_version": SCHEMA, "run_id": run_id, "runs": runs}
    dump(output / "report.json", result)
    (output / "report.md").write_text(markdown(result), encoding="utf-8")
    return result


def inside(root, name):
    candidate = root / name
    resolved = candidate.resolve()
    if Path(name).is_absolute() or not resolved.is_relative_to(root):
        raise ValueError(f"path outside installation: {name}")
    if candidate != resolved:
        raise ValueError(f"noncanonical installation path: {name}")
    return resolved


def check_binary(path, system, *, library=False):
    with path.open("rb") as stream:
        header = stream.read(64)
        if system == "linux":
            valid = (header[:6] == b"\x7fELF\x01\x01" and len(header) >= 20
                     and header[18:20] == b"\x03\x00"
                     and (not library or header[16:18] == b"\x03\x00"))
        else:
            valid = False
            if header[:2] == b"MZ" and len(header) == 64:
                stream.seek(struct.unpack_from("<I", header, 60)[0])
                pe = stream.read(26)
                valid = (len(pe) == 26 and pe[:6] == b"PE\0\0\x4c\x01"
                         and pe[24:26] == b"\x0b\x01"
                         and (not library or bool(pe[23] & 0x20)))
    if not valid:
        raise ValueError(f"expected 32-bit x86 {system} binary: {path}")


def version_metadata():
    def git(*args):
        return subprocess.check_output(["git", *args], cwd=REPO)
    try:
        revision = git("rev-parse", "HEAD").decode().strip()
        return {
            "audit_plugin_commit": revision,
            "sdk_commit": revision,
            "working_tree_dirty": bool(git("status", "--porcelain", "--untracked-files=normal")),
        }
    except (OSError, subprocess.CalledProcessError):
        return {"audit_plugin_commit": None, "sdk_commit": None}


def label(value, what):
    if not re.fullmatch(r"[A-Za-z0-9_-]{1,64}", value):
        raise ValueError(f"invalid {what}: {value!r}")
    return value


class Console:
    def __init__(self, process, output):
        self.process = process
        self.output = output
        self.threads = []
        self.condition = threading.Condition()
        self.events = []
        self.errors = []
        self.truncated = []
        for name in ("stdout", "stderr"):
            thread = threading.Thread(target=self.read, args=(name,), daemon=True)
            thread.start()
            self.threads.append(thread)

    def read(self, name):
        try:
            with (self.output / f"{name}.log").open("wb") as log:
                stream = getattr(self.process, name)
                remaining = LOG_LIMIT
                while line := stream.readline(16384):
                    written = log.write(line[:remaining])
                    remaining -= written
                    if len(line) > written:
                        if name not in self.truncated:
                            self.truncated.append(name)
                    event = parse_event(line)
                    if event:
                        with self.condition:
                            if len(self.events) < MAX_EVENTS * 2:
                                self.events.append(event)
                            self.condition.notify_all()
        except OSError as error:
            self.errors.append(str(error))

    def sequence(self):
        with self.condition:
            return max((event["seq"] for event in self.events), default=0)

    def wait(self, kind, timeout, after=0, **fields):
        deadline = time.monotonic() + timeout
        with self.condition:
            while True:
                for event in self.events:
                    if event["seq"] > after and event["kind"] == kind and all(event.get(k) == v for k, v in fields.items()):
                        return event
                remaining = deadline - time.monotonic()
                if remaining <= 0 or self.process.poll() is not None:
                    raise RuntimeError(f"did not observe {kind}; exit={self.process.poll()}")
                self.condition.wait(min(remaining, 0.25))

    def send(self, line):
        data = (line + "\n").encode("ascii")
        if len(data) > 512:
            raise ValueError("console command exceeds bounded write")
        self.process.stdin.write(data)
        self.process.stdin.flush()


def stop(process, console, timeout):
    if process.poll() is not None:
        return "already exited"
    try:
        console.send("quit")
        process.wait(timeout=timeout)
        return "quit"
    except (OSError, subprocess.TimeoutExpired):
        if process.poll() is None:
            if os.name == "posix":
                try:
                    os.killpg(process.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
            else:
                process.terminate()
        try:
            process.wait(timeout=5)
            return "terminated"
        except subprocess.TimeoutExpired:
            if os.name == "posix":
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            else:
                process.kill()
            process.wait(timeout=5)
            return "killed"


def configure(stage, spec):
    mod = inside(stage, spec["mod"])
    liblist = mod / "liblist.gam"
    audit_dir = mod / "addons" / "metamod-audit"
    if audit_dir.exists():
        raise ValueError("installation already contains the reserved addons/metamod-audit directory")
    audit_dir.mkdir(parents=True)
    system = spec["os"]
    name = "metamod_audit.dll" if system == "windows" else "libmetamod_audit.so"
    deployed_plugin = audit_dir / name
    shutil.copy2(spec["_plugin"], deployed_plugin)
    manager = inside(stage, spec["metamod"]["binary"]).relative_to(mod).as_posix()
    game = inside(stage, spec["gamedll"]["binary"]).relative_to(mod).as_posix()
    if not all(re.fullmatch(r"[A-Za-z0-9_./-]+", path) for path in (manager, game)):
        raise ValueError("manager and GameDLL paths must be console-safe paths inside the mod")
    original = liblist.read_bytes()
    key = b"gamedll" if system == "windows" else b"gamedll_linux"
    replacement = key + b' "' + manager.encode("ascii") + b'"'
    patched, count = re.subn(rb"(?im)^[ \t]*" + key + rb"[ \t]+[^\r\n]*", replacement, original)
    if count != 1:
        raise ValueError("liblist.gam must contain exactly one active platform GameDLL entry")
    liblist.write_bytes(patched)
    plugin_file = "addons/metamod-audit/plugins.ini"
    (mod / plugin_file).write_text(f"{'win32' if system == 'windows' else 'linux'} addons/metamod-audit/{name}\n", encoding="ascii")
    # An isolated Metamod configuration prevents copied options from
    # redirecting logging, plugin loading, or GameDLL resolution.
    config_file = "addons/metamod-audit/config.ini"
    (mod / config_file).write_text(f"gamedll {game}\n", encoding="ascii")
    return config_file, plugin_file, game, deployed_plugin


def preserve_dumps(stage, output):
    saved = []
    for path in stage.rglob("*"):
        if len(saved) >= 8:
            break
        if path.is_symlink() or not path.is_file():
            continue
        if path.suffix.casefold() not in {".mdmp", ".dmp", ".core"} and path.name != "core" and not path.name.startswith("core."):
            continue
        if path.stat().st_size > 256 * 1024 * 1024:
            continue
        target = output / f"dump-{len(saved)}-{path.name}"
        shutil.copy2(path, target)
        saved.append(target.name)
    return saved


def sample_rss(process, checkpoint):
    try:
        text = Path(f"/proc/{process.pid}/status").read_text(encoding="ascii")
        rss = re.search(r"^VmRSS:\s+(\d+) kB$", text, re.M)
        return {"checkpoint": checkpoint, "rss_kib": int(rss[1]) if rss else None}
    except (OSError, UnicodeError):
        return {"checkpoint": checkpoint, "rss_kib": None}


def run_one(spec, config_dir, output, run_id, allow_build, client_wait, prepare_only=False):
    runtime = {key: spec[key] for key in ("id", "os") if key in spec}
    runtime["architecture"] = "i686" if spec.get("os") == "linux" else "x86"
    runtime["host"] = {"system": platform.system(), "release": platform.release(), "machine": platform.machine()}
    runtime["components"] = {}
    runtime["experimental"] = spec.get("experimental", False)
    metadata = {"run_id": run_id, "executed": False, "execution": "MISSING", "exit_status": None}
    if reason := spec.get("unsupported_reason"):
        runtime["unsupported_reason"] = reason
        metadata.update(execution="UNSUPPORTED", error=reason)
        return aggregate(output, runtime, metadata, [])
    try:
        system = spec["os"]
        if system not in {"linux", "windows"}:
            raise ValueError(f"unsupported OS {system}")
        if platform.system().lower() != system:
            raise ValueError(f"requires a {system} host")
        installation = (config_dir / spec["installation"]).resolve(strict=True)
        if output.resolve().is_relative_to(installation):
            raise ValueError("output directory is inside the source installation")
        mod = inside(installation, spec["mod"])
        label(spec["mod"], "mod directory")
        if not (mod / "liblist.gam").is_file():
            raise ValueError("mod is missing liblist.gam")
        for component in ("engine", "metamod", "gamedll"):
            config = spec[component]
            if (not config["implementation"].strip() or not config["version"].strip()
                    or config["version"] == "SET_EXACT_VERSION"):
                raise ValueError(f"{component} requires an independently identified implementation and exact version")
            path = inside(installation, config["binary"])
            check_binary(path, system, library=component != "engine")
            actual_hash = digest(path)
            if config.get("sha256", actual_hash).lower() != actual_hash:
                raise ValueError(f"{component} hash differs from the configured artifact")
            runtime["components"][component] = {**config, "sha256": actual_hash, "version_source": "operator configuration"}
        # Some engines use a launcher and a separate engine module. Hash any
        # such modules independently instead of conflating them with the exe.
        modules = {}
        for name in spec["engine"].get("modules", []):
            module = inside(installation, name)
            check_binary(module, system, library=True)
            modules[name] = digest(module)
        runtime["components"]["engine"]["modules"] = modules
        lib_dirs = spec["engine"].get("library_dirs", [])
        if not all(inside(installation, path).is_dir() for path in lib_dirs):
            raise ValueError("engine.library_dirs must point to directories within the installation")
        maps = spec["maps"]
        if not 1 <= len(maps) <= 8:
            raise ValueError("configure one to eight maps")
        for name in maps:
            label(name, "map")
            if not (mod / "maps" / (name + ".bsp")).is_file() and not (installation / "valve" / "maps" / (name + ".bsp")).is_file():
                raise ValueError(f"map is missing: {name}")
        port = int(spec.get("port", 27045))
        if not 1024 <= port <= 65535:
            raise ValueError("port must be in 1024..65535")
        timeout = int(spec.get("timeout_seconds", 45))
        if not 1 <= timeout <= 300:
            raise ValueError("timeout_seconds must be in 1..300")
        target = "i686-unknown-linux-gnu" if system == "linux" else "i686-pc-windows-msvc"
        plugin_name = "libmetamod_audit.so" if system == "linux" else "metamod_audit.dll"
        if "plugin" in spec:
            plugin = (config_dir / spec["plugin"]).resolve(strict=True)
            provenance = {key: spec.get(key) for key in ("audit_plugin_commit", "sdk_commit")}
        elif allow_build:
            provenance = version_metadata()
            build_target = output.resolve() / "build"
            command = ["cargo", "build", "--locked", "--release", "--target", target, "-p", "metamod-audit", "--target-dir", str(build_target)]
            with (output / "build.log").open("wb") as log:
                subprocess.run(command, cwd=REPO, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=600)
            plugin = build_target / target / "release" / plugin_name
        else:
            plugin = REPO / "target" / target / "release" / plugin_name
            provenance = {key: spec.get(key) for key in ("audit_plugin_commit", "sdk_commit")}
        check_binary(plugin, system, library=True)
        plugin_hash = digest(plugin)
        if spec.get("plugin_sha256", plugin_hash).lower() != plugin_hash:
            raise ValueError("plugin hash differs from the configured artifact")
        runtime["plugin"] = {**provenance, "sha256": plugin_hash, "target": target}
        # Dereferencing symlinks during copy could escape the source tree.
        # Reject junctions too, including on Python versions without is_junction.
        for directory, dirs, files in os.walk(installation):
            for name in dirs + files:
                entry = Path(directory) / name
                stat = entry.lstat()
                if entry.is_symlink() or getattr(stat, "st_file_attributes", 0) & 0x400:
                    raise ValueError(f"source installation contains a symlink or reparse point: {entry}")
                if not entry.is_dir() and not entry.is_file():
                    raise ValueError(f"source installation contains a special file: {entry}")
        container = nullcontext(output) if prepare_only else tempfile.TemporaryDirectory(prefix="goldsrc-audit-", dir=output)
        with container as temp:
            stage = Path(temp).resolve() / "server"
            shutil.copytree(installation, stage)
            settings, plugins, game, deployed_plugin = configure(stage, {**spec, "_plugin": plugin})
            for identity in runtime["components"].values():
                if digest(inside(stage, identity["binary"])) != identity["sha256"]:
                    raise ValueError(f"source changed while staging {identity['binary']}")
            for name, expected_hash in modules.items():
                if digest(inside(stage, name)) != expected_hash:
                    raise ValueError(f"source changed while staging {name}")
            if digest(deployed_plugin) != plugin_hash:
                raise ValueError("plugin changed while staging")
            binary = inside(stage, spec["engine"]["binary"])
            args = [str(binary), "-console", "-insecure", "-nomaster", "-game", spec["mod"],
                    "-port", str(port), "+ip", "127.0.0.1", "+sv_lan", "1", "+deathmatch", "1",
                    "+localinfo", "mm_configfile", settings, "+localinfo", "mm_pluginsfile", plugins,
                    "+localinfo", "mm_gamedll", game, "+map", maps[0]]
            env = os.environ.copy()
            launch_env = {}
            if system == "linux":
                launch_env["LD_LIBRARY_PATH"] = os.pathsep.join([str(stage), *(str(inside(stage, path)) for path in lib_dirs)])
            elif lib_dirs:
                launch_env["PATH"] = os.pathsep.join([*(str(inside(stage, path)) for path in lib_dirs), env["PATH"]])
            env.update(launch_env)
            if prepare_only:
                args.insert(2, "-condebug")
            dump(output / "launch.json", {"argv": args, "cwd": str(stage), "environment": launch_env})
            if prepare_only:
                metadata["execution"] = "PREPARED"
                dump(output / "runtime.json", runtime)
                return aggregate(output, runtime, metadata, [])
            with subprocess.Popen(args, cwd=stage, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                  stderr=subprocess.PIPE, bufsize=0, start_new_session=(system == "linux")) as process:
                console = Console(process, output)
                metadata.update(executed=True, execution="INCOMPLETE")
                metadata["resource_samples"] = []
                try:
                    console.wait("ready", timeout, submitted=True)
                    before = console.sequence()
                    console.send("version")
                    console.send("meta version")
                    console.send("meta list")
                    console.send("mm_audit_status")
                    console.wait("status", timeout, after=before)
                    metadata["resource_samples"].append(sample_rss(process, "ready"))
                    for index, name in enumerate(maps):
                        if index:
                            before = console.sequence()
                            console.send(f"changelevel {name}")
                            console.wait("result", timeout, after=before, test="map.activate.v1")
                        before = console.sequence()
                        console.send('mm_audit_run all "audit two words"')
                        console.wait("suite_complete", timeout, after=before, suite="all")
                        metadata["resource_samples"].append(sample_rss(process, f"suite:{name}"))
                    if client_wait:
                        print(f"{runtime['id']}: connect to 127.0.0.1:{port} now; waiting {client_wait} seconds", flush=True)
                        deadline = time.monotonic() + client_wait
                        while time.monotonic() < deadline and process.poll() is None:
                            time.sleep(min(1, max(0, deadline - time.monotonic())))
                    before = console.sequence()
                    console.send("mm_audit_report")
                    console.wait("report", timeout, after=before)
                    metadata["execution"] = "COMPLETE"
                except (OSError, RuntimeError) as error:
                    metadata["error"] = str(error)
                finally:
                    metadata["shutdown_method"] = stop(process, console, min(timeout, 15))
                    metadata["exit_status"] = process.returncode
                    for thread in console.threads:
                        thread.join(timeout=5)
                    metadata["capture_errors"] = console.errors + [
                        "console stream did not close" for thread in console.threads if thread.is_alive()
                    ]
                    metadata["truncated_streams"] = console.truncated
                if (process.returncode != 0 or metadata["shutdown_method"] != "quit"
                        or metadata["capture_errors"] or console.truncated):
                    metadata["execution"] = "INCOMPLETE"
            metadata["crash_dumps"] = preserve_dumps(stage, output)
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError) as error:
        metadata["error"] = str(error)
        if not metadata["executed"]:
            metadata["execution"] = "UNAVAILABLE"
    logs = [path for path in (output / "stdout.log", output / "stderr.log") if path.exists()]
    return aggregate(output, runtime, metadata, logs)


def compare(baseline, candidate, allowances):
    def indexed(report):
        return {(run["runtime"]["id"], test["test"]): test["result"]
                for run in report["runs"] for test in run["tests"]}
    before, after = indexed(baseline), indexed(candidate)
    differences = []
    for key in sorted(before.keys() | after.keys()):
        old, new = before.get(key), after.get(key)
        if old == new:
            continue
        runtime, test = key
        reason = next((rule["reason"] for rule in allowances
                       if (rule["runtime"], rule["test"], rule["before"], rule["after"]) == (runtime, test, old, new)), None)
        category = ("expected" if reason else "regression" if old == "PASS" and new == "FAIL"
                    else "coverage loss" if old in {"PASS", "FAIL"} and new not in {"PASS", "FAIL"}
                    else "change")
        differences.append({"runtime": runtime, "test": test, "before": old, "after": new,
                            "category": category, "reason": reason})
    return differences


def compare_runtimes(report, allowances):
    runs = report["runs"]
    differences = []
    for index, left in enumerate(runs):
        for right in runs[index + 1:]:
            left_id, right_id = left["runtime"]["id"], right["runtime"]["id"]
            old = {test["test"]: test for test in left["tests"]}
            new = {test["test"]: test for test in right["tests"]}
            for test in sorted(old.keys() & new.keys()):
                before, after = old[test]["result"], new[test]["result"]
                if before == after:
                    if (before == "PASS" and test in {"lag.post.v1", "player.identity.v1", "trace.line.v1", "trace.hull.v1"}
                            and old[test]["observed"] != new[test]["observed"]):
                        differences.append({"left": left_id, "right": right_id, "test": test,
                                            "before": old[test]["observed"], "after": new[test]["observed"],
                                            "category": "observation difference", "reason": "both probes passed"})
                    continue
                reason = next((rule["reason"] for rule in allowances
                               if (rule["left"], rule["right"], rule["test"], rule["before"], rule["after"])
                               == (left_id, right_id, test, before, after)), None)
                differences.append({"left": left_id, "right": right_id, "test": test,
                                    "before": before, "after": after,
                                    "category": "expected" if reason else "runtime difference", "reason": reason})
    return differences


def load(path):
    return json.loads(path.read_text(encoding="utf-8"))


def matrix_template(control):
    runtimes = []
    for system in ("linux", "windows"):
        linux = system == "linux"
        for engine, engine_name in (("hlds", "Valve HLDS"), ("rehlds", "ReHLDS")):
            for manager, manager_name in (("classic", "alliedmodders/metamod-hl1"), ("metamod-r", "Metamod-R")):
                games = [("stock", "ricochet", "Stock Ricochet"),
                         ("updated", "ricochet", "ricochet-updated")]
                if control:
                    games.append(("hldm", "valve", "Half-Life Deathmatch"))
                for game, mod, game_name in games:
                    game_binary = {
                        "stock": "ricochet.so" if linux else "mp.dll",
                        "updated": "ricochet_updated.so" if linux else "ricochet_updated.dll",
                        "hldm": "hl.so" if linux else "hl.dll",
                    }[game]
                    runtimes.append({
                        "id": f"{system}-{engine}-{manager}-{game}",
                        "os": system, "installation": f"installations/{system}-{engine}",
                        "mod": mod, "experimental": engine == "hlds" and manager == "metamod-r",
                        "engine": {"implementation": engine_name, "version": "SET_EXACT_VERSION",
                                   "binary": "hlds_linux" if linux else "hlds.exe",
                                   "modules": ["engine_i486.so" if linux else "swds.dll"]},
                        "metamod": {"implementation": manager_name, "version": "SET_EXACT_VERSION",
                                    "binary": f"{mod}/addons/{manager}/dlls/" + ("metamod_i386.so" if linux else "metamod.dll")},
                        "gamedll": {"implementation": game_name, "version": "SET_EXACT_VERSION",
                                    "binary": f"{mod}/dlls/{game_binary}"},
                        "maps": ["crossfire", "boot_camp"] if mod == "valve" else ["rc_arena", "rc_deathmatch"],
                    })
    return {"schema_version": SCHEMA, "runtimes": runtimes}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    template = sub.add_parser("template", help="print a matrix configuration with editable paths and versions")
    template.add_argument("--hldm", action="store_true")
    run = sub.add_parser("run")
    run.add_argument("config", type=Path)
    run.add_argument("output", type=Path)
    run.add_argument("--only", action="append", default=[])
    run.add_argument("--platform", type=str.lower, choices=("linux", "windows"))
    run.add_argument("--build", action="store_true")
    run.add_argument("--prepare-only", action="store_true", help="keep an isolated copy for manual console operation")
    run.add_argument("--client-wait", type=int, choices=range(0, 601), default=0, metavar="SECONDS")
    imp = sub.add_parser("import")
    imp.add_argument("runtime", type=Path, help="independently identified runtime JSON")
    imp.add_argument("output", type=Path)
    imp.add_argument("logs", nargs="+", type=Path)
    imp.add_argument("--manual", type=Path)
    diff = sub.add_parser("compare")
    diff.add_argument("baseline", type=Path)
    diff.add_argument("candidate", nargs="?", type=Path)
    diff.add_argument("--allowances", type=Path)
    args = parser.parse_args()
    run_id = str(uuid.uuid4())
    try:
        if args.command == "template":
            print(json.dumps(matrix_template(args.hldm), indent=2))
            return 0
        if args.command == "compare":
            allowances = load(args.allowances) if args.allowances else []
            baseline = load(args.baseline)
            changes = (compare(baseline, load(args.candidate), allowances) if args.candidate
                       else compare_runtimes(baseline, allowances))
            print(json.dumps(changes, indent=2))
            return int(any(change["category"] in {"regression", "coverage loss"} for change in changes))
        if args.command == "run":
            config_path = args.config.resolve(strict=True)
            config = load(config_path)
            if config["schema_version"] != SCHEMA:
                raise ValueError("unsupported configuration schema")
            specs = config["runtimes"]
            ids = [label(spec["id"], "runtime ID") for spec in specs]
            if len(ids) != len(set(ids)) or not set(args.only).issubset(ids):
                raise ValueError("duplicate runtime IDs or unknown --only selection")
            specs = [spec for spec in specs if (not args.only or spec["id"] in args.only)
                     and (not args.platform or spec["os"] == args.platform)]
            if not specs:
                raise ValueError("no runtimes configured")
            output_path = args.output.resolve()
            for spec in specs:
                source = (config_path.parent / spec["installation"]).resolve()
                if output_path.is_relative_to(source):
                    raise ValueError("output must be outside each source installation")
            args.output.mkdir(parents=True)
            runs = []
            for spec in specs:
                output = (args.output / spec["id"]).resolve()
                output.mkdir()
                runs.append(run_one(spec, config_path.parent, output, run_id, args.build, args.client_wait, args.prepare_only))
                finish(args.output, run_id, runs)
            if args.prepare_only:
                return int(any(run["execution"] != "PREPARED" for run in runs))
        else:
            runtime = load(args.runtime)
            label(runtime["id"], "runtime ID")
            records = manual_results(args.manual)
            args.output.mkdir(parents=True)
            metadata = {"run_id": run_id, "execution": "IMPORTED", "executed": True,
                        "exit_status": runtime.get("exit_status"), "original_logs": [str(path) for path in args.logs]}
            runs = [aggregate(args.output.resolve(), runtime, metadata, args.logs, records)]
            finish(args.output, run_id, runs)
        return int(any(run["execution"] not in {"COMPLETE", "IMPORTED"} or not run["protocol_complete"]
                       or run["diagnostics"] or run["protocol_warnings"]
                       or run["exit_status"] not in {None, 0}
                       or any(test.get("result") == "FAIL" or test.get("manual_conflict")
                              for test in run["tests"] + run["contract_findings"])
                       for run in runs))
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.exit(2, f"audit: {error}\n")


if __name__ == "__main__":
    raise SystemExit(main())

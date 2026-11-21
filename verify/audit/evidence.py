"""Interpret bounded console and process evidence without guessing component identities."""

import ntpath
import os
from pathlib import Path
import re

NATIVE_PREFIX = b"MM_AUDIT_NATIVE "
CONSOLE_PREFIX = r"^\s*(?:L\s+\d{2}/\d{2}/\d{4}\s+-\s+\d{2}:\d{2}:\d{2}:\s*)?(?:\[META(?:MOD)?\]\s*)?"
VERSION = r"([0-9]+(?:\.[0-9]+)+(?:[-+_][0-9A-Za-z.+_-]+)?)"
MANAGER = re.compile(CONSOLE_PREFIX + r"(Metamod(?:[- ]R)?)\s+(?:version\s*[:=]?\s*|v\s*)?"
                     + VERSION + r"(?=\s|,|$)", re.I)
ENGINE = re.compile(CONSOLE_PREFIX + r"(ReHLDS\s+version|Exe\s+version)\s*[:=]?\s*"
                    + VERSION + r"(/[0-9A-Za-z_-]+)?(?=\s|\(|$)", re.I)
BUILD = re.compile(CONSOLE_PREFIX + r"(?:Exe\s+build|Build\s+date)\s*:\s*.*\((\d+)\)\s*$", re.I)
GAME_PATH = re.compile(CONSOLE_PREFIX + r"(?:dll\s+pathname|gamedll\s+path(?:name)?)\s*:\s*(.+?)\s*$", re.I)
# Classic and Metamod-R's static list truncate description/file columns.
# Match the status column after the known description, not any RUN token.
PLUGIN_ROW = re.compile(CONSOLE_PREFIX + r"\[\s*\d+\s*\]\s+Rust Runtime Au(?:dit)?\s+"
                        r"(RUN|RUNNING|PAUS|PAUSED|FAIL|BADF|OPEN|INFO|EMPT|UNK\d+)\s+\S+\s+"
                        r"(?:libmetamod_audit(?:\.so)?|metamod_audit\.dll?)(?=\s|$)", re.I)
SANITIZER = re.compile(r"(?:Address|Leak|UndefinedBehavior|Thread|Memory)Sanitizer\b|\bruntime error:", re.I)
CRASH = re.compile(r"segmentation fault|access violation|Host_Error|fatal signal|core dumped", re.I)
CONSOLE_EVIDENCE = re.compile(
    r"Protocol\s+version|Exe\s+version|Exe\s+build|Build\s+date|Build\s+from|ReHLDS\s+version|Metamod(?:[- ]R)?\s+(?:version|build:|from:|v?\d)"
    r"|dll\s+pathname|gamedll\s+path|Rust Runtime Au|metamod_audit", re.I
)


def linux_mappings(pid, expected):
    """Correlate mapped device/inode with the staged artifacts, not basenames."""
    try:
        present = set()
        named = set()
        with Path(f"/proc/{pid}/maps").open(encoding="utf-8", errors="replace") as stream:
            for line in stream:
                _, permissions, _, device, inode, *pathname = line.rstrip("\n").split(maxsplit=5)
                major, minor = (int(value, 16) for value in device.split(":"))
                if inode != "0" and "x" in permissions:
                    present.add((major, minor, int(inode)))
                    if pathname and pathname[0].startswith("/"):
                        named.add((pathname[0], int(inode)))
        result = {}
        for component, paths in expected.items():
            members = {}
            for path in paths:
                stat = path.stat()
                # Overlayfs can report different device numbers for stat
                # and mappings. Require the full staged pathname and inode
                # when the device number does not match.
                members[str(path)] = ((os.major(stat.st_dev), os.minor(stat.st_dev), stat.st_ino) in present
                                      or (str(path), stat.st_ino) in named)
            result[component] = members
        return result
    except (OSError, ValueError):
        return {}


def windows_path(path):
    path = ntpath.normcase(ntpath.normpath(path))
    if path.startswith("\\\\?\\unc\\"):
        return "\\\\" + path[8:]
    if path.startswith("\\\\?\\"):
        return path[4:]
    return path


def windows_mappings(pid, expected):
    """Use Toolhelp's 32-bit module view. Access errors leave evidence unavailable."""
    import ctypes
    from ctypes import wintypes as win

    class ModuleEntry(ctypes.Structure):
        _fields_ = [
            ("size", win.DWORD), ("module_id", win.DWORD), ("process_id", win.DWORD),
            ("global_usage", win.DWORD), ("process_usage", win.DWORD),
            ("base", win.LPVOID), ("base_size", win.DWORD), ("module", win.HMODULE),
            ("name", win.WCHAR * 256), ("path", win.WCHAR * 260),
        ]

    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    snapshot = kernel.CreateToolhelp32Snapshot
    snapshot.argtypes = (win.DWORD, win.DWORD)
    snapshot.restype = win.HANDLE
    first, advance = kernel.Module32FirstW, kernel.Module32NextW
    for fn in (first, advance):
        fn.argtypes = (win.HANDLE, ctypes.POINTER(ModuleEntry))
        fn.restype = win.BOOL
    kernel.CloseHandle.argtypes = (win.HANDLE,)
    kernel.CloseHandle.restype = win.BOOL
    for _ in range(3):
        handle = snapshot(0x08 | 0x10, pid)  # TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32
        if handle != ctypes.c_void_p(-1).value or ctypes.get_last_error() != 24:  # ERROR_BAD_LENGTH
            break
    if handle == ctypes.c_void_p(-1).value:
        return {}
    entry = ModuleEntry()
    entry.size = ctypes.sizeof(entry)
    try:
        if not first(handle, ctypes.byref(entry)):
            return {}
        paths = set()
        while True:
            paths.add(windows_path(entry.path))
            if not advance(handle, ctypes.byref(entry)):
                if ctypes.get_last_error() != 18:  # ERROR_NO_MORE_FILES
                    return {}
                break
        return {name: {str(path): windows_path(str(path)) in paths for path in files}
                for name, files in expected.items()}
    finally:
        kernel.CloseHandle(handle)


def identity(runtime, lines, events, mappings):
    """Keep runtime observations separate from the configured artifact metadata."""
    components = runtime.get("components", {})
    root = runtime.get("staged_root")
    windows = runtime.get("os") == "windows"
    path_ops = ntpath if windows else os.path
    normalize = windows_path if windows else os.path.normpath
    managers = {("metamod-r" if name.lower().endswith(("-r", " r")) else "classic", version)
                for line in lines for name, version in MANAGER.findall(line)}
    engine_tokens = {(" ".join(name.lower().split()), version + variant)
                     for line in lines for name, version, variant in ENGINE.findall(line)}
    engine_tokens.update(("exe build", build) for line in lines for build in BUILD.findall(line))
    statuses = {status.upper() for line in lines for status in PLUGIN_ROW.findall(line)}
    plugin_listed = bool(statuses & {"RUN", "RUNNING"})
    plugin_replied = any(event["kind"] == "status" for event in events)
    game_paths = [path.strip(' "\'') for line in lines for path in GAME_PATH.findall(line)]
    results = {}

    for name in ("engine", "metamod", "gamedll", "plugin"):
        config = components.get(name, {})
        mapped = mappings.get(name, {})
        mapped_ok = bool(mapped) and all(mapped.values())
        reasons = []
        tokens = set()
        implementation_ok = False
        if name == "engine":
            configured = config.get("implementation", "").lower()
            label = "rehlds version" if "rehlds" in configured else "exe version"
            tokens = {value for field, value in engine_tokens if field == label}
            if config.get("console_version_field") == "exe build":
                tokens = {value for field, value in engine_tokens if field == "exe build"}
            if len(tokens) > 1:
                reasons.append("conflicting engine versions in console output")
            has_rehlds = any(label == "rehlds version" for label, _ in engine_tokens)
            if has_rehlds and "hlds" in configured and "rehlds" not in configured:
                reasons.append("ReHLDS reported by a configured Valve HLDS row")
            implementation_ok = (has_rehlds if "rehlds" in configured else mapped_ok and bool(tokens))
        elif name == "metamod":
            configured = config.get("implementation", "").lower()
            expected_family = "metamod-r" if "metamod-r" in configured else "classic"
            tokens = {version for _, version in managers}
            families = {family for family, _ in managers}
            if config and families - {expected_family}:
                reasons.append("unexpected Metamod family in console version output")
            if len(tokens) > 1:
                reasons.append("conflicting Metamod versions in console output")
            implementation_ok = expected_family in families
        elif name == "gamedll":
            expected = normalize(path_ops.join(root, config["binary"])) if root and config.get("binary") else None
            absolute = {normalize(path) for path in game_paths
                        if path_ops.isabs(path) and (not windows or ntpath.splitdrive(path)[0])}
            if expected and absolute - {expected}:
                reasons.append("reported GameDLL path differs from staged file")
            implementation_ok = mapped_ok or (plugin_replied and expected is not None and expected in absolute)
        else:
            implementation_ok = plugin_replied and (mapped_ok or plugin_listed)
            if statuses - {"RUN", "RUNNING"}:
                reasons.append("the audit plugin has a non-running Metamod status")

        expected_version = config.get("console_version")
        if expected_version is None and name in {"engine", "metamod"}:
            supplied = config.get("version", "")
            if re.fullmatch(r"v?\d+(?:[.+_-][0-9A-Za-z]+)*", supplied):
                expected_version = supplied.removeprefix("v")
        if name == "engine" and expected_version and "/" not in expected_version:
            tokens = {value.split("/", 1)[0] for value in tokens}
        if expected_version and tokens and expected_version not in tokens:
            reasons.append(f"expected console version {expected_version} was not observed")
        if mapped and not mapped_ok:
            reasons.append("one or more staged artifacts were absent from process mappings")
        if name in {"engine", "metamod"}:
            sufficient = implementation_ok and mapped_ok and bool(expected_version) and expected_version in tokens
        else:
            sufficient = implementation_ok
        results[name] = {
            "result": "FAIL" if reasons else "PASS" if sufficient else "INCONCLUSIVE",
            "reasons": reasons or ([] if sufficient else ["insufficient runtime evidence"]),
            "expected_console_version": expected_version,
            "observed_versions": sorted(tokens),
            "mapped_artifacts": mapped,
        }
    results["engine"]["version_fields"] = sorted(engine_tokens)
    results["metamod"]["version_banners"] = sorted(managers)
    results["gamedll"]["reported_paths"] = game_paths
    results["plugin"].update(listed_running=plugin_listed, listed_statuses=sorted(statuses), protocol_reply=plugin_replied)
    return results

import json
import os
from pathlib import Path
import platform
import tempfile
import unittest
from unittest.mock import patch

import runner


def wire(seq, kind, **fields):
    return runner.PREFIX + json.dumps({"schema_version": 1, "seq": seq, "kind": kind, **fields}).encode() + b"\n"


def result(seq, test, status):
    return runner.parse_event(wire(seq, "result", test=test, result=status, expected="expected", observed="evidence"))


def by_test(results):
    return {test["test"]: test for test in results}


class ProtocolTests(unittest.TestCase):
    def test_template_covers_independent_components(self):
        matrix = runner.matrix_template(False)["runtimes"]
        self.assertEqual(len(matrix), 16)
        self.assertEqual(len({row["id"] for row in matrix}), 16)
        self.assertEqual(sum(row["experimental"] for row in matrix), 4)
        self.assertEqual(len(runner.matrix_template(True)["runtimes"]), 24)

    def test_parser_rejects_unknown_schema_and_malformed_records(self):
        self.assertIsNone(runner.parse_event(b'MM_AUDIT garbage'))
        self.assertIsNone(runner.parse_event(wire(1, "unknown")))
        self.assertIsNone(runner.parse_event(wire(True, "report")))
        self.assertIsNone(runner.parse_event(wire(4097, "report")))
        self.assertEqual(runner.parse_event(b"[server] " + wire(2, "report", complete=True))["seq"], 2)

    def test_out_of_order_streams_and_partial_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            a, b = folder / "out", folder / "err"
            a.write_bytes(wire(2, "report", complete=True) + b"MM_AUDIT {broken\n")
            b.write_bytes(wire(1, "status") + b"AddressSanitizer: crash\n")
            report = runner.aggregate(folder, {"id": "fixture"}, {"executed": True, "run_id": "run"}, [a, b])
            events = [json.loads(line) for line in (folder / "events.jsonl").read_text().splitlines()]
            self.assertEqual([event["seq"] for event in events], [1, 2])
            self.assertEqual(events[0]["run_id"], "run")
            self.assertFalse(report["protocol_complete"])
            self.assertTrue(report["diagnostics"])
            self.assertTrue(report["protocol_warnings"])

    def test_runtime_identity_needs_independent_evidence(self):
        runtime = {
            "os": "linux", "staged_root": "/stage",
            "components": {
                "engine": {"implementation": "Valve HLDS", "version": "1.1.2.7"},
                "metamod": {"implementation": "alliedmodders/metamod-hl1", "version": "1.21.1-am"},
                "gamedll": {"binary": "ricochet/dlls/updated.so"},
            },
        }
        lines = ["Exe version 1.1.2.7 (ricochet)", "Metamod v1.21.1-am",
                 "dll pathname: /stage/ricochet/dlls/updated.so"]
        mappings = {"engine": {"engine.so": True}, "metamod": {"manager.so": True}}
        events = [{"kind": "status"}]
        verified = runner.identity(runtime, lines, events, mappings)
        self.assertEqual(verified["engine"]["result"], "PASS")
        self.assertEqual(verified["metamod"]["result"], "PASS")
        self.assertEqual(verified["gamedll"]["result"], "PASS")
        self.assertEqual(verified["plugin"]["result"], "INCONCLUSIVE")
        listed = runner.identity(runtime, lines + ["[ 1] Rust Runtime Audit RUN"], events, mappings)
        self.assertEqual(listed["plugin"]["result"], "PASS")
        contradicted = runner.identity(runtime, lines + ["ReHLDS version: 3.14.0.857",
                                       "Metamod-r version 1.3.0.149", "dll pathname: /elsewhere/updated.so"],
                                       events, mappings)
        for name in ("engine", "metamod", "gamedll"):
            self.assertEqual(contradicted[name]["result"], "FAIL")
        self.assertEqual(runner.identity(runtime, ["Exe version 1.1.2.8"], events, mappings)
                         ["engine"]["result"], "FAIL")

    def test_native_evidence_remains_separate_and_protocol_detects_loss(self):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            log = folder / "console"
            native = (b'MM_AUDIT_NATIVE {"schema_version":1,"test":"native.hook.result.v1",'
                      b'"result":"PASS","observed":"asserted fixture result"}\n')
            log.write_bytes(wire(1, "status") + wire(3, "suite_complete", suite="all")
                            + wire(4, "report", complete=True) + wire(4, "report", complete=True)
                            + b'MM_AUDIT {"seq":5' + b"\n" + native)
            common = {"executed": True, "run_id": "run"}
            real = runner.aggregate(folder, {"id": "real"}, common, [log])
            synthetic = runner.aggregate(folder, {"id": "fixture", "evidence_scope": "native-fixture"}, common, [log])
            self.assertFalse(real["protocol_complete"])
            self.assertTrue(any("duplicate" in warning for warning in real["protocol_warnings"]))
            self.assertTrue(any("missing" in warning for warning in real["protocol_warnings"]))
            self.assertFalse(real["native_fixture_results"])
            self.assertEqual(synthetic["native_fixture_results"][0]["source"], "native-harness")
            self.assertFalse(synthetic["runtime_identity"])

    def test_health_regressions_and_host_hash_drift(self):
        def report(**fields):
            return {"runs": [{"runtime": {"id": "host", "components": {"engine": {"sha256": fields.pop("sha", "a")}}},
                              "tests": [{"test": "same.v1", "result": "PASS"}], **fields}]}

        baseline = report(exit_status=0, protocol_complete=True)
        candidate = report(exit_status=-11, protocol_complete=False, sha="b",
                           sanitizer_diagnostics=["ERROR: AddressSanitizer"],
                           crash_diagnostics=["segmentation fault"],
                           truncated_streams=["stdout"], capture_errors=["pipe closed"])
        deltas = {row["field"]: row["category"] for row in runner.compare(baseline, candidate, [])}
        self.assertEqual(deltas["host_configuration"], "configuration drift")
        for field in ("exit_status", "protocol_complete", "sanitizer_diagnostics",
                      "crash_diagnostics", "truncated_streams", "capture_errors"):
            self.assertEqual(deltas[field], "regression")
        with self.assertRaises(ValueError):
            runner.compare(baseline, candidate, [{"reason": " "}])

    def test_sanitizer_evidence_survives_complete_protocol(self):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            log = folder / "log"
            log.write_bytes(wire(1, "ready", submitted=True) + wire(2, "status")
                            + wire(3, "result", test="init.v1", result="PASS", expected="init", observed="ran")
                            + wire(4, "suite_complete", suite="all") + wire(5, "report", complete=True)
                            + b"==42==ERROR: LeakSanitizer: detected memory leaks\n")
            run = runner.aggregate(folder, {"id": "runtime"}, {"executed": True, "run_id": "run"}, [log])
            self.assertTrue(run["protocol_complete"])
            self.assertEqual(by_test(run["tests"])["init.v1"]["result"], "PASS")
            self.assertEqual(len(run["sanitizer_diagnostics"]), 1)

    def test_conflicting_duplicate_does_not_erase_failure_or_later_diagnostics(self):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            log = folder / "log"
            log.write_bytes(wire(1, "result", test="init.v1", result="PASS", expected="init", observed="first")
                            + wire(1, "result", test="init.v1", result="FAIL", expected="init", observed="conflict")
                            + b"segmentation fault\n")
            run = runner.aggregate(folder, {"id": "runtime"}, {"executed": True, "run_id": "run"}, [log])
            self.assertFalse(run["protocol_complete"])
            self.assertEqual(by_test(run["tests"])["init.v1"]["result"], "FAIL")
            self.assertEqual(run["crash_diagnostics"], ["segmentation fault"])

    def test_missing_and_manual_evidence_do_not_hide_failure(self):
        expected = [
            result(1, "init.v1", "PASS"),
            result(2, "map.activate.v1", "FAIL"),
            result(3, "map.activate.v1", "PASS"),
        ]
        manual = [
            {"test": "map.activate.v1", "result": "PASS", "observed": "manual observation"},
            {"test": "hook.result.v1", "result": "PASS", "observed": "isolated fixture"},
        ]
        tests = by_test(runner.findings(expected, True, manual))
        self.assertEqual(tests["map.activate.v1"]["result"], "FAIL")
        self.assertEqual(tests["map.activate.v1"]["event_sequences"], [2, 3])
        self.assertEqual(tests["map.activate.v1"]["manual_conflict"], "PASS")
        self.assertEqual(tests["hook.result.v1"]["source"], "manual")
        self.assertEqual(tests["shutdown.v1"]["result"], "SKIP")
        self.assertEqual(by_test(runner.findings([], False, []))["init.v1"]["observed"], "runtime did not execute")

    def test_comparison_separates_regression_coverage_and_allowance(self):
        def report(status):
            return {"runs": [{"runtime": {"id": "fixture"}, "tests": [{"test": "a.v1", "result": status}]}]}

        baseline = report("PASS")
        self.assertEqual(runner.compare(baseline, report("FAIL"), [])[0]["category"], "regression")
        self.assertEqual(runner.compare(baseline, report("SKIP"), [])[0]["category"], "coverage loss")
        allowance = {"runtime": "fixture", "test": "a.v1", "before": "PASS", "after": "SKIP", "reason": "fixture disabled"}
        self.assertEqual(runner.compare(baseline, report("SKIP"), [allowance])[0]["category"], "expected")


@unittest.skipUnless(platform.system() == "Linux", "fake launcher uses a POSIX shebang")
class ExecutionTests(unittest.TestCase):
    def test_linux_mappings_use_file_identity(self):
        executable = Path("/proc/self/exe").resolve(strict=True)
        with tempfile.NamedTemporaryFile() as unmapped:
            other = Path(unmapped.name)
            mapped = runner.linux_mappings(os.getpid(), {"engine": [executable], "plugin": [other]})
        self.assertTrue(mapped["engine"][str(executable)])
        self.assertFalse(mapped["plugin"][str(other)])

    def test_isolated_stage_process_and_capture(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            mod = source / "ricochet"
            (mod / "maps").mkdir(parents=True)
            for map_name in ("rc_arena", "rc_deathmatch"):
                (mod / "maps" / f"{map_name}.bsp").touch()
            liblist = mod / "liblist.gam"
            original = b'gamedll "dlls/ricochet.dll"\r\ngamedll_linux "stock.so"\r\n'
            liblist.write_bytes(original)
            original_plugins = mod / "plugins.ini"
            original_plugins.write_text("original")
            for name in ("meta.so", "stock.so"):
                (mod / name).touch()
            (root / "libmetamod_audit.so").touch()
            engine = source / "hlds_linux"
            engine.write_text("""#!/usr/bin/env python3
import json
import sys
seq = 0
def emit(kind, **fields):
    global seq
    seq += 1
    print("MM_AUDIT " + json.dumps(dict(schema_version=1, seq=seq, kind=kind, **fields)), flush=True)
assert b'gamedll_linux "meta.so"' in open("ricochet/liblist.gam", "rb").read()
assert open("ricochet/plugins.ini").read() == "original"
emit("ready", submitted=True)
for line in sys.stdin:
    if line.startswith("version"):
        print("Exe version fixture 1.0", flush=True)
    elif line.startswith("mm_audit_status"):
        emit("status")
    elif line.startswith("changelevel"):
        emit("result", test="map.deactivate.v1", result="PASS", expected="deactivation", observed="fake callback")
        emit("result", test="map.activate.v1", result="PASS", expected="activation", observed="fake callback")
    elif line == 'mm_audit_run all "audit two words"\\n':
        emit("suite_complete", suite="all")
    elif line.startswith("mm_audit_report"):
        emit("report", complete=True)
    elif line.startswith("quit"):
        emit("result", test="shutdown.v1", result="PASS", expected="shutdown", observed="fake callback")
        break
""")
            engine.chmod(0o755)
            output = root / "report"
            output.mkdir()
            spec = {
                "id": "fake", "os": "linux", "installation": "source", "mod": "ricochet",
                "maps": ["rc_arena", "rc_deathmatch"], "map_cycles": 2, "timeout_seconds": 2,
                "plugin": "libmetamod_audit.so",
                "engine": {"implementation": "fake", "version": "1", "binary": "hlds_linux"},
                "metamod": {"implementation": "fake", "version": "2", "binary": "ricochet/meta.so"},
                "gamedll": {"implementation": "fake", "version": "3", "binary": "ricochet/stock.so"},
            }
            # The fixture uses a script and placeholder libraries. Keep the
            # real process, staging, stdin and report path under test.
            with patch.object(runner, "check_binary"):
                record = runner.run_one(spec, root, output, "fixture-run", False, 0)
                missing = runner.run_one({**spec, "installation": "missing"}, root, output, "fixture-run", False, 0)
                unsupported = runner.run_one({**spec, "unsupported_reason": "fixture exclusion"}, root, output,
                                             "fixture-run", False, 0)
                manual_output = root / "manual"
                manual_output.mkdir()
                prepared = runner.run_one(spec, root, manual_output, "fixture-run", False, 0, prepare_only=True)
            self.assertEqual(record["execution"], "COMPLETE", record)
            self.assertEqual(record["exit_status"], 0)
            self.assertEqual([t["result"] for t in record["map_transitions"]], ["PASS"] * 3)
            self.assertEqual(by_test(record["tests"])["shutdown.v1"]["result"], "PASS")
            self.assertEqual(record["console_version_observations"], ["Exe version fixture 1.0"])
            self.assertEqual(liblist.read_bytes(), original)
            self.assertEqual(original_plugins.read_text(), "original")
            self.assertFalse(list(output.glob("goldsrc-audit-*")))
            self.assertEqual(missing["execution"], "UNAVAILABLE")
            self.assertFalse(missing["executed"])
            self.assertEqual(unsupported["execution"], "UNSUPPORTED")
            self.assertEqual(by_test(unsupported["tests"])["init.v1"]["classification"], "Unsupported configuration")
            self.assertEqual(prepared["execution"], "PREPARED")
            self.assertFalse(prepared["executed"])
            launch = json.loads((manual_output / "launch.json").read_text())
            self.assertIn("-condebug", launch["argv"])
            self.assertTrue((Path(launch["cwd"]) / "ricochet/addons/metamod-audit/plugins.ini").exists())

            crash_output = root / "crash"
            crash_output.mkdir()
            engine.write_text(engine.read_text().replace('emit("report", complete=True)', 'emit("report", complete=True); sys.exit(19)'))
            with patch.object(runner, "check_binary"):
                crash = runner.run_one(spec, root, crash_output, "fixture-run", False, 0)
            self.assertEqual(crash["execution"], "INCOMPLETE")
            self.assertEqual(crash["exit_status"], 19)
            self.assertTrue(crash["protocol_complete"])
            self.assertIn('\"kind\": \"ready\"', (crash_output / "events.jsonl").read_text())


if __name__ == "__main__":
    unittest.main()

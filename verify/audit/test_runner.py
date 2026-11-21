import json
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
        self.assertEqual(runner.parse_event(b"[server] " + wire(2, "report"))["seq"], 2)

    def test_out_of_order_streams_and_partial_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            a, b = folder / "out", folder / "err"
            a.write_bytes(wire(2, "report") + b"MM_AUDIT {broken\n")
            b.write_bytes(wire(1, "status") + b"AddressSanitizer: crash\n")
            report = runner.aggregate(folder, {"id": "fixture"}, {"executed": True, "run_id": "run"}, [a, b])
            events = [json.loads(line) for line in (folder / "events.jsonl").read_text().splitlines()]
            self.assertEqual([event["seq"] for event in events], [1, 2])
            self.assertEqual(events[0]["run_id"], "run")
            self.assertFalse(report["protocol_complete"])
            self.assertTrue(report["diagnostics"])
            self.assertTrue(report["protocol_warnings"])

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
        emit("result", test="map.activate.v1", result="PASS", expected="activation", observed="fake callback")
    elif line == 'mm_audit_run all "audit two words"\\n':
        emit("suite_complete", suite="all")
    elif line.startswith("mm_audit_report"):
        emit("report")
    elif line.startswith("quit"):
        emit("result", test="shutdown.v1", result="PASS", expected="shutdown", observed="fake callback")
        break
""")
            engine.chmod(0o755)
            output = root / "report"
            output.mkdir()
            spec = {
                "id": "fake", "os": "linux", "installation": "source", "mod": "ricochet",
                "maps": ["rc_arena", "rc_deathmatch"], "timeout_seconds": 2,
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
                manual_output = root / "manual"
                manual_output.mkdir()
                prepared = runner.run_one(spec, root, manual_output, "fixture-run", False, 0, prepare_only=True)
            self.assertEqual(record["execution"], "COMPLETE", record)
            self.assertEqual(record["exit_status"], 0)
            self.assertEqual(by_test(record["tests"])["shutdown.v1"]["result"], "PASS")
            self.assertEqual(record["console_version_observations"], ["Exe version fixture 1.0"])
            self.assertEqual(liblist.read_bytes(), original)
            self.assertEqual(original_plugins.read_text(), "original")
            self.assertFalse(list(output.glob("goldsrc-audit-*")))
            self.assertEqual(missing["execution"], "UNAVAILABLE")
            self.assertFalse(missing["executed"])
            self.assertEqual(prepared["execution"], "PREPARED")
            self.assertFalse(prepared["executed"])
            launch = json.loads((manual_output / "launch.json").read_text())
            self.assertIn("-condebug", launch["argv"])
            self.assertTrue((Path(launch["cwd"]) / "ricochet/addons/metamod-audit/plugins.ini").exists())

            crash_output = root / "crash"
            crash_output.mkdir()
            engine.write_text(engine.read_text().replace('emit("status")', 'sys.exit(19)'))
            with patch.object(runner, "check_binary"):
                crash = runner.run_one(spec, root, crash_output, "fixture-run", False, 0)
            self.assertEqual(crash["execution"], "INCOMPLETE")
            self.assertEqual(crash["exit_status"], 19)
            self.assertFalse(crash["protocol_complete"])
            self.assertIn("did not observe status", crash["error"])
            self.assertIn('\"kind\": \"ready\"', (crash_output / "events.jsonl").read_text())


if __name__ == "__main__":
    unittest.main()

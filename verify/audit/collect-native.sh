#!/usr/bin/env bash
# CI wrapper for the existing Linux ABI and loader verification.
set -euo pipefail

output=$1
fixture=$2
mkdir -p "$output"
status=0
bash verify/linux-i686.sh > "$output/loader.log" 2>&1 || status=$?
cat "$output/loader.log"
printf '{"id":"%s","os":"linux","architecture":"i686","exit_status":%d}\n' \
    "$fixture" "$status" > "$output/runtime.json"
python3 verify/audit/runner.py import "$output/runtime.json" "$output/report" \
    "$output/loader.log" --native-fixture --require-native-suite

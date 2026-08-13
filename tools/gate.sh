#!/usr/bin/env bash
# GAP-14 (docs/audit/gaps.md) / round-5 plan Task 6
# (docs/plan/v0.2-class-closure-round5.local.md): mechanises the
# release-gate-5 depth rule instead of leaving it as a convention someone
# has to remember. The rule was written 2026-08-10 and sat unexercised for
# three days before it was finally run for real — which immediately found
# BUG-59. This script is the "ask" that rule needed: run it, and the depth
# is in the output, not just in a reviewer's head.
#
# Usage: tools/gate.sh
# Requires: REQUIRE_IVERILOG-capable toolchain (real iverilog/vvp on PATH).
set -uo pipefail

DEPTH=5000

echo "== release gate 5: differential fuzz at ${DEPTH}/${DEPTH} (GAP-14 mandated depth, not the 400/400 per-PR depth) =="

export REQUIRE_IVERILOG=1
export MIMZ_DIFF_FUZZ_N="$DEPTH"
export MIMZ_DIFF_FUZZ_CLOCKED_N="$DEPTH"

# Deliberately NOT --release (round-5 plan Task 7): measured locally,
# N=100 was 25.7s in debug vs 29.5s in release for this exact test — debug
# is dominated by iverilog/vvp subprocess time, not rustc's -O, so a debug
# build costs nothing here and keeps `hoist_if_needed`'s `debug_assert!`
# self-checks live at the depth that actually matters.
#
# No `set -e` here on purpose — the script must reach the pass/fail banner
# below regardless of `cargo test`'s exit code, then propagate it.
cargo test --test differential_fuzz --no-fail-fast
status=$?

if [ "$status" -eq 0 ]; then
  echo "== gate 5: CLEAN at ${DEPTH}/${DEPTH} =="
else
  echo "== gate 5: FAILED at ${DEPTH}/${DEPTH} — do not score this gate green =="
fi

exit "$status"

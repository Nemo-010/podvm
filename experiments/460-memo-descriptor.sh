#!/bin/sh
# Question: does `podvm run -d` and `run --rm` work when the parent holds
# descriptors 3 to N open, for each N up to 16?
#
# TODO/enter.md T-1914 (PodUtils/podrunner#177). podvm passes the memo on
# descriptor 17 of the payload, and pod-enter refuses a root directory
# that holds that number in podvm; with 3 to 15 held, podvm
# 0.2.0-beta.22 opened its root on 17 and refused ("a passed descriptor
# goes onto 17").
#
# Pinned inputs:
#   podvm   PODVM_460_BIN (default the release build of the workspace)
#   image   mirror.gcr.io/library/alpine:3.20
#
# Clauses:
#   1. for each N in 2..16, `run -d IMG true` and `run --rm IMG true` exit 0
#      with descriptors 3..N held by the parent, and print no "goes onto".

set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN="${PODVM_460_BIN:-$ROOT/target/x86_64-unknown-linux-musl/release/podvm}"
OUT="$ROOT/experiments/results/memo-descriptor.txt"
IMG=mirror.gcr.io/library/alpine:3.20
fail=0
: >"$OUT"
say() { printf '%s\n' "$*" | tee -a "$OUT"; }
say "date      $(date -u +%Y-%m-%dT%H:%M:%SZ)"
say "podvm     $("$BIN" version 2>/dev/null)"
timeout 300 "$BIN" pull -q "$IMG" >/dev/null 2>&1
row=""
for top in 2 4 6 8 10 12 13 14 15 16; do
	out=$(bash -c "for i in \$(seq 3 $top); do eval \"exec \$i</dev/null\"; done
		id=\$(timeout 60 '$BIN' run -d '$IMG' true) || exit 9
		timeout 60 '$BIN' rm -f \"\$id\" >/dev/null 2>&1
		timeout 60 '$BIN' run --rm '$IMG' true" 2>&1)
	rc=$?
	if [ "$rc" -ne 0 ] || printf '%s' "$out" | grep -q "goes onto"; then
		row="$row $top:FAIL"
		fail=1
		say "held 3..$top: rc $rc: $(printf '%s' "$out" | tail -1)"
	else
		row="$row $top:ok"
	fi
done
say "sweep    $row"
[ "$fail" -eq 0 ] && say "PASS" || say "FAIL"
exit "$fail"

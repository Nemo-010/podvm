#!/bin/sh
# Question: does `podvm run` of a caller with no capability
# enter the namespace rung through a user namespace that podvm opens, with
# a /proc of its own, `--hostname`, the exit code of the payload, the
# shutdown signals, and no process after the run?
#
# TODO/probe.md T-1909 (PodUtils/podvm#397). Run it as a user that is not
# uid 0, on a host where `unshare -Urm true` exits 0.
#
# Pinned inputs:
#   podvm   PODVM_458_BIN (default the release build of the workspace)
#   image   mirror.gcr.io/library/alpine:3.20
#   store   a fresh HOME under .work/exp458, or under PODVM_458_WORK
#
# Clauses:
#   1. `run --rm --verbose` names mode=namespace and the user namespace;
#   2. /proc/self/uid_map of the payload maps 0 to the uid of the caller;
#   3. `--hostname unit1` gives `unit1` for `uname -n`;
#   4. `exit 7` of the payload gives exit 7;
#   5. SIGTERM to podvm, with a payload that has a background child, gives
#      143 and no process of the payload stays;
#   6. `run -d` keeps the entry as before (no user namespace) and exits 0;
#   7. `PODVM_USERNS=0` keeps the entry as before: mode=userland.

set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN="${PODVM_458_BIN:-$ROOT/target/x86_64-unknown-linux-musl/release/podvm}"
WORK="${PODVM_458_WORK:-$ROOT/.work/exp458}"
OUT="$ROOT/experiments/results/userns-entry.txt"
IMG=mirror.gcr.io/library/alpine:3.20
if [ "$(id -u)" -eq 0 ]; then
	echo "SKIP: run as a user that is not uid 0" >&2
	exit 2
fi
if ! unshare -Urm true 2>/dev/null; then
	echo "SKIP: this host opens no user namespace for this user" >&2
	exit 2
fi
rm -rf "$WORK"
mkdir -p "$WORK/home"
export HOME="$WORK/home"
unset PODVM_USERNS
unset PODVM_STORE
fail=0
: >"$OUT"
say() { printf '%s\n' "$*" | tee -a "$OUT"; }
bad() { say "FAIL: $*"; fail=1; }

say "date      $(date -u +%Y-%m-%dT%H:%M:%SZ)"
say "podvm     $("$BIN" version 2>/dev/null)"
say "kernel    $(uname -r), uid $(id -u)"
timeout 300 "$BIN" pull -q "$IMG" >/dev/null 2>&1

b=$(timeout 120 "$BIN" run --rm --verbose "$IMG" true 2>&1)
say "banner    $(printf '%s\n' "$b" | grep -o 'mode=[a-z]*' | head -1); $(printf '%s\n' "$b" | grep -c 'opened a user') user line"
printf '%s\n' "$b" | grep -q 'mode=namespace' && printf '%s\n' "$b" | grep -q 'opened a user' || bad "clause 1"

m=$(timeout 120 "$BIN" run --rm "$IMG" cat /proc/self/uid_map 2>/dev/null | tr -s ' ' | sed 's/^ //')
say "uid_map   $m"
[ "$m" = "0 $(id -u) 1" ] || bad "clause 2"

h=$(timeout 120 "$BIN" run --rm --hostname unit1 "$IMG" uname -n 2>/dev/null)
say "hostname  $h"
[ "$h" = unit1 ] || bad "clause 3"

timeout 120 "$BIN" run --rm "$IMG" sh -c 'exit 7' >/dev/null 2>&1
rc=$?
say "exit      $rc"
[ "$rc" -eq 7 ] || bad "clause 4"

timeout 120 "$BIN" run --rm "$IMG" sh -c 'sleep 301 & sleep 301' >/dev/null 2>&1 &
sleep 8
outer=$(pgrep -u "$(id -u)" -f "$BIN run --rm $IMG sh -c sleep 301" | sort -n | head -1)
kill -TERM "$outer" 2>/dev/null
wait "$!" 2>/dev/null
rc=$?
sleep 1
stayed=$(pgrep -u "$(id -u)" -f "sleep 301" | wc -l)
say "sigterm   rc $rc, $stayed process(es) stayed"
[ "$rc" -eq 143 ] && [ "$stayed" -eq 0 ] || bad "clause 5"
pkill -9 -u "$(id -u)" -f "sleep 301" 2>/dev/null

id=$(timeout 120 "$BIN" run -d "$IMG" sleep 30 2>/dev/null)
rc=$?
timeout 60 "$BIN" rm -f "$id" >/dev/null 2>&1
say "detached  rc $rc"
[ "$rc" -eq 0 ] || bad "clause 6"

b=$(PODVM_USERNS=0 timeout 120 "$BIN" run --rm --verbose "$IMG" true 2>&1)
say "off       $(printf '%s\n' "$b" | grep -o 'mode=[a-z]*' | head -1)"
printf '%s\n' "$b" | grep -q 'opened a user' && bad "clause 7"

chmod -R u+w "$WORK" 2>/dev/null
rm -rf "$WORK"
[ "$fail" -eq 0 ] && say "PASS" || say "FAIL"
exit "$fail"

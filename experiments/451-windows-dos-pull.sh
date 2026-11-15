#!/bin/sh
# Question: does `podvm pull windows:dos` take FreeDOS end to end on a
# host with no KVM, does `images` list it, does `run windows:dos ver`
# boot it under TCG and give exit 0, and does `rmi` remove it?
#
# TODO/machine.md T-1861. The catalog pins the FreeDOS 1.4 LiteUSB zip
# (size and sha256); `pull` fetches and checks it and writes the base
# disk; `run` boots the base in a disposable guest. One guest runs at a
# time, each with a bound, and no emulator may stay.
#
# Pinned inputs:
#   podvm   PODVM_451_BIN (default the release build of the workspace)
#   store   a fresh HOME and PODVM_STORE under .work/exp451
#
# Clauses:
#   1. `podvm pull windows:dos` exits 0 and prints a path that exists;
#   2. `podvm images` lists windows:dos;
#   3. `podvm run windows:dos ver` exits 0 and prints the FreeCOM version;
#   4. `podvm rmi windows:dos` exits 0, and `images` lists it no more;
#   5. no emulator stays.

set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN="${PODVM_451_BIN:-$ROOT/target/x86_64-unknown-linux-musl/release/podvm}"
WORK="$ROOT/.work/exp451"
OUT="$ROOT/experiments/results/windows-dos-pull.txt"
rm -rf "$WORK"
mkdir -p "$WORK/home"
export HOME="$WORK/home" PODVM_STORE="$WORK/store"
fail=0
: >"$OUT"
say() { printf '%s\n' "$*" | tee -a "$OUT"; }
bad() { say "FAIL: $*"; fail=1; }

say "date      $(date -u +%Y-%m-%dT%H:%M:%SZ)"
say "podvm     $("$BIN" version 2>/dev/null)"
say "accel     $([ -r /dev/kvm ] && echo kvm || echo tcg)"

t0=$(date +%s)
path=$(timeout 900 "$BIN" pull windows:dos 2>"$WORK/pull.err")
rc=$?
say "pull      rc $rc in $(( $(date +%s) - t0 )) s: $path"
[ "$rc" -eq 0 ] && [ -e "$path" ] || { bad "clause 1"; tail -3 "$WORK/pull.err" | tee -a "$OUT"; }

"$BIN" images >"$WORK/images.txt" 2>&1
grep -q "dos" "$WORK/images.txt" || bad "clause 2: images lists no windows:dos"
say "images    $(grep dos "$WORK/images.txt" | head -1)"

t0=$(date +%s)
out=$(timeout 900 "$BIN" run windows:dos ver 2>"$WORK/run.err")
rc=$?
say "run ver   rc $rc in $(( $(date +%s) - t0 )) s: $(printf '%s' "$out" | tr -d '\r' | grep -i version | head -1)"
[ "$rc" -eq 0 ] && printf '%s' "$out" | grep -qi "version" || { bad "clause 3"; tail -3 "$WORK/run.err" | tee -a "$OUT"; }

"$BIN" rmi windows:dos >"$WORK/rmi.txt" 2>&1
rc=$?
"$BIN" images >"$WORK/images2.txt" 2>&1
say "rmi       rc $rc"
[ "$rc" -eq 0 ] && ! grep -q "dos" "$WORK/images2.txt" || bad "clause 4"

if pgrep -a qemu-system >/dev/null; then
	bad "clause 5: an emulator stayed"
	pkill -9 qemu-system
fi
rm -rf "$WORK"
[ "$fail" -eq 0 ] && say "PASS" || say "FAIL"
exit "$fail"

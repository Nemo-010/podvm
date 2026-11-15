#!/bin/sh
# Question: does `podvm pull windows:NAME` install a setup ISO of the
# catalog into a golden image under KVM, how long does it take, is the
# golden image standalone, is the ISO unchanged, and does no emulator
# stay?
#
# TODO/machine.md T-1867. `pull` fetches and checks the ISO, boots it with
# the answer stick on a blank 64 GiB disk, sends Enter for the prompt of
# the firmware, watches the phases, and converts the disk into
# `windows-NAME.golden.qcow2` in the store. One guest runs at a time, with
# the bound of `timeout`.
#
# Pinned inputs:
#   podvm   PODVM_452_BIN (default the release build of the workspace)
#   names   PODVM_452_NAMES (default "2025")
#   store   HOME (the caller gives a disk with room: about 60 GB)
#
# Clauses, for each name:
#   1. `podvm pull windows:NAME` exits 0 and prints the golden image;
#   2. `qemu-img info` names no backing file of the golden image;
#   3. the sha256 of the ISO in the store is the pin of the catalog;
#   4. no emulator stays.
# With PODVM_452_REFUSAL=1, the clause of a host with TCG only:
#   5. `podvm pull windows:2025` exits 125 at once, names `--accel tcg`,
#      and fetches nothing.

set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN="${PODVM_452_BIN:-$ROOT/target/x86_64-unknown-linux-musl/release/podvm}"
NAMES="${PODVM_452_NAMES:-2025}"
OUT="$ROOT/experiments/results/windows-install.txt"
STORE="$HOME/.local/share/podvm/windows/catalog"
fail=0
say() { printf '%s\n' "$*" | tee -a "$OUT"; }
bad() { say "FAIL: $*"; fail=1; }

if [ "${PODVM_452_REFUSAL:-0}" = 1 ]; then
	say "refusal  $(date -u +%Y-%m-%dT%H:%M:%SZ), /dev/kvm: $(ls -l /dev/kvm 2>&1)"
	t0=$(date +%s)
	err=$(env -u PODVM_MACHINE_ACCEL timeout 120 "$BIN" pull windows:2025 2>&1 >/dev/null)
	rc=$?
	say "refusal  rc $rc in $(( $(date +%s) - t0 )) s: $err"
	[ "$rc" -eq 125 ] && printf '%s' "$err" | grep -q -- "--accel tcg" || bad "clause 5: no refusal that names --accel tcg"
	[ -e "$STORE/win2025-eval.iso" ] && bad "clause 5: the refusal fetched the ISO"
	exit "$fail"
fi

: >"$OUT"
say "date     $(date -u +%Y-%m-%dT%H:%M:%SZ)"
say "podvm    $("$BIN" version 2>/dev/null)"
say "accel    $([ -r /dev/kvm ] && [ -w /dev/kvm ] && echo kvm || echo tcg)"
say "disk     $(df -h "$HOME" | tail -1)"

for name in $NAMES; do
	t0=$(date +%s)
	golden=$(timeout 170m "$BIN" pull "windows:$name" 2>"$ROOT/experiments/results/windows-install-$name.err")
	rc=$?
	secs=$(( $(date +%s) - t0 ))
	say "$name     pull rc $rc in ${secs} s ($(( secs / 60 )) min): $golden"
	tail -25 "$ROOT/experiments/results/windows-install-$name.err" | tee -a "$OUT"
	if [ "$rc" -ne 0 ] || [ ! -f "$golden" ]; then
		bad "clause 1 ($name)"
	else
		info=$(qemu-img info "$golden" 2>&1)
		say "$name     $(printf '%s\n' "$info" | grep -E '^(virtual size|disk size)' | tr '\n' ' ')"
		printf '%s' "$info" | grep -qi "backing file" && bad "clause 2 ($name): the golden image names a backing file"
	fi
	iso=$(ls "$STORE"/*.iso 2>/dev/null | head -1)
	pin=$(grep -A12 "name: \"$name\"" "$ROOT/crates/podvm-windows/src/catalog.rs" | sed -n 's/.*sha256: "\([0-9a-f]*\)".*/\1/p' | head -1)
	if [ -n "$iso" ]; then
		got=$(sha256sum "$iso" | cut -d' ' -f1)
		say "$name     iso sha256 $got"
		[ "$got" = "$pin" ] || bad "clause 3 ($name): the ISO changed (pin $pin)"
	fi
	if pgrep -a qemu-system >/dev/null; then
		bad "clause 4 ($name): an emulator stayed"
		pkill -9 qemu-system
	fi
	# The next name needs the room of this one.
	rm -f "$STORE"/*.iso
	"$BIN" rmi "windows:$name" >/dev/null 2>&1 || true
done
[ "$fail" -eq 0 ] && say "PASS" || say "FAIL"
exit "$fail"

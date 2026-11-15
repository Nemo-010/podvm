#!/bin/sh
# Question: does `podvm run --podvm-tier=machine --platform linux/arm64`
# boot an aarch64 guest on an x86_64 host under TCG, give the payload's
# stdout, stderr and exit code, and leave no emulator?
#
# TODO/machine.md T-1890. The emulator is the static qemu-system-aarch64
# 9.2.0 of pkgforge (`ppkg.qemu`) in a relocatable layout with the
# `efi-virtio.rom` of QEMU v9.2.0; the kernel is `boot/vmlinuz-virt` of
# alpine-netboot-3.22.6-aarch64.tar.gz, which podvm fetches itself on the
# first run and checks against the pin of `alpine-netboot-aarch64` of
# pod-services (podcrates v0.51.0, COORD-128). Each file is checked
# against its sha256. One guest at a time, bounded.
#
# Pinned inputs:
#   podvm   PODVM_456_BIN (default the release build of the workspace)
#   image   mirror.gcr.io/library/alpine:3.20 (linux/arm64)
#
# Clauses:
#   1. `uname -m` prints aarch64 and exits 0;
#   2. `sh -c 'echo out; echo err >&2; exit 3'` gives `out` on stdout,
#      `err` on stderr and exit 3;
#   3. PODVM_MACHINE_ACCEL=kvm is refused by name (exit 125);
#   4. no emulator stays;
#   5. the store holds the pinned kernel (machine/arm64/vmlinuz-dd37c446...).

set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN="${PODVM_456_BIN:-$ROOT/target/x86_64-unknown-linux-musl/release/podvm}"
WORK="$ROOT/.work/exp456"
OUT="$ROOT/experiments/results/machine-arm64.txt"
IMG=mirror.gcr.io/library/alpine:3.20
QEMU_URL='https://api.ghcr.pkgforge.dev/pkgforge/bincache/qemu/ppkg/qemu-system-aarch64?tag=9.2.0-x86_64-linux&download=qemu-system-aarch64'
QEMU_SHA=1ed86579cf74ecbe4d118b098d96d3e127beebde048b3d8e5bc907781403531b
ROM_URL=https://raw.githubusercontent.com/qemu/qemu/v9.2.0/pc-bios/efi-virtio.rom
ROM_SHA=26be36901db7f8181c306cc62bd74891d8646528965a78e40cceadba5dd7c8e7
KERNEL_SHA=dd37c4460f933ec909ff022c34944ff7e1a5300aff99750e3bdc02cd4074c332
rm -rf "$WORK"
mkdir -p "$WORK/q/bin" "$WORK/q/share/qemu"
fail=0
: >"$OUT"
say() { printf '%s\n' "$*" | tee -a "$OUT"; }
bad() { say "FAIL: $*"; fail=1; }
pinned() { [ "$(sha256sum "$1" | cut -d' ' -f1)" = "$2" ] || { say "COULD NOT RUN: $1 is not the pin"; exit 2; }; }

say "date      $(date -u +%Y-%m-%dT%H:%M:%SZ)"
say "podvm     $("$BIN" version 2>/dev/null)"
say "host      $(uname -m)"
curl -qfsSL -o "$WORK/q/bin/qemu-system-aarch64" "$QEMU_URL" || { say "COULD NOT RUN: the emulator"; exit 2; }
pinned "$WORK/q/bin/qemu-system-aarch64" "$QEMU_SHA"
chmod +x "$WORK/q/bin/qemu-system-aarch64"
curl -qfsSL -o "$WORK/q/share/qemu/efi-virtio.rom" "$ROM_URL" || { say "COULD NOT RUN: the ROM"; exit 2; }
pinned "$WORK/q/share/qemu/efi-virtio.rom" "$ROM_SHA"
say "emulator  $("$WORK/q/bin/qemu-system-aarch64" --version | head -1)"
export PATH="$WORK/q/bin:$PATH" HOME="$WORK/home" PODVM_STORE="$WORK/store"
unset PODVM_MACHINE_KERNEL_ARM64
timeout 600 "$BIN" pull -q --platform linux/arm64 "$IMG" >/dev/null 2>&1

t0=$(date +%s)
out=$(timeout 600 "$BIN" run --rm --podvm-tier=machine --platform linux/arm64 "$IMG" uname -m 2>"$WORK/1.err")
rc=$?
say "uname -m  rc $rc in $(( $(date +%s) - t0 )) s: $out"
[ "$rc" -eq 0 ] && [ "$out" = aarch64 ] || { bad "clause 1"; tail -3 "$WORK/1.err" | tee -a "$OUT"; }

t0=$(date +%s)
timeout 600 "$BIN" run --rm --podvm-tier=machine --platform linux/arm64 "$IMG" sh -c 'echo out; echo err >&2; exit 3' >"$WORK/2.out" 2>"$WORK/2.err"
rc=$?
say "streams   rc $rc in $(( $(date +%s) - t0 )) s: stdout [$(tr '\n' ' ' <"$WORK/2.out")] stderr [$(tr '\n' ' ' <"$WORK/2.err")]"
[ "$rc" -eq 3 ] && [ "$(cat "$WORK/2.out")" = out ] && grep -qx err "$WORK/2.err" || bad "clause 2"

err=$(PODVM_MACHINE_ACCEL=kvm timeout 60 "$BIN" run --rm --podvm-tier=machine --platform linux/arm64 "$IMG" true 2>&1)
rc=$?
say "kvm       rc $rc: $err"
[ "$rc" -eq 125 ] && printf '%s' "$err" | grep -q "TCG" || bad "clause 3"

if pgrep -f "qemu-system-aarch64" >/dev/null; then
	bad "clause 4: an emulator stayed"
	pkill -9 -f qemu-system-aarch64
fi
k=$(ls "$WORK/store/machine/arm64/" 2>/dev/null | grep -c "^vmlinuz-${KERNEL_SHA%"${KERNEL_SHA#????????????????}"}$")
say "kernel    the store holds $k pinned kernel(s)"
[ "$k" -eq 1 ] || bad "clause 5"
rm -rf "$WORK"
[ "$fail" -eq 0 ] && say "PASS" || say "FAIL"
exit "$fail"

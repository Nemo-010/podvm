#!/bin/sh
# Question: does `run --podvm-tier=machine` under KVM pass for a user with
# no root that `sudo -u` makes, as it passes for the user of the runner?
#
# PodUtils/podrunner#195 (COORD-069 of podrunner), TODO/machine.md T-1922.
# On a hosted runner, the lab user of podrunner (made with `sudo -u USER
# --preserve-env=PATH,TMPDIR`, a fresh HOME, no root, /dev/kvm 0666) got
# exit 125 and "the guest ended before it gave the exit code of the
# payload", with the chardev notice of QEMU as the only line. The user of
# the runner passes the same command. Run it as the user of a GitHub
# runner, with passwordless sudo and /dev/kvm 0666 (the job `nonroot` of
# .github/workflows/kvm.yml).
#
# Pinned inputs:
#   podvm   PODVM_463_BIN (default the release build of the workspace)
#   image   public.ecr.aws/docker/library/alpine:3.20 at its digest (412)
#
# Clauses:
#   1. as the user of the runner, `run --rm --podvm-tier=machine
#      --podvm-mem 4G IMAGE true` exits 0;
#   2. as a new user through `sudo -u`, with a fresh HOME and its own store,
#      the same command exits 0;
#   3. as that user, with --podvm-mem 1G, it exits 0;
#   4. no emulator stays after the runs.
# Each failed clause keeps its stderr, the limits of the user, and the end
# of the kernel log in the transcript.

set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN="${PODVM_463_BIN:-$ROOT/target/x86_64-unknown-linux-musl/release/podvm}"
OUT="$ROOT/experiments/results/machine-nonroot.txt"
IMG='public.ecr.aws/docker/library/alpine:3.20@sha256:d9e853e87e55526f6b2917df91a2115c36dd7c696a35be12163d44e6e2a4b6bc'
LAB=podvmlab
if [ ! -r /dev/kvm ] || [ ! -w /dev/kvm ] || ! sudo -n true 2>/dev/null; then
	echo "SKIP: no usable /dev/kvm or no passwordless sudo" >&2
	exit 2
fi
fail=0
mkdir -p "$(dirname "$OUT")"
: >"$OUT"
say() { printf '%s\n' "$*" | tee -a "$OUT"; }
bad() { say "FAIL: $*"; fail=1; }
keep() { sed 's/^/    /' "$1" | tee -a "$OUT"; }
kernel() { sudo dmesg 2>/dev/null | tail -15 | sed 's/^/    dmesg: /' | tee -a "$OUT"; }

# The binary goes where each user can run it, as an installer puts it.
sudo install -m 0755 "$BIN" /usr/local/bin/podvm-463
P=/usr/local/bin/podvm-463
say "date      $(date -u +%Y-%m-%dT%H:%M:%SZ)"
say "podvm     $($P version 2>/dev/null)"
say "runner    uid $(id -u), groups $(id -Gn)"
say "kvm       $(ls -l /dev/kvm)"

# Clause 1: the user of the runner.
timeout 600 "$P" run --verbose --rm --podvm-tier=machine --podvm-mem 4G "$IMG" true \
	>/dev/null 2>"$OUT.1.err"
rc=$?
say "clause 1  rc $rc (the user of the runner)"
[ "$rc" -eq 0 ] || { bad "clause 1"; keep "$OUT.1.err"; kernel; }

# Clauses 2 and 3: a new user through sudo, as podrunner's nonroot.sh.
id "$LAB" >/dev/null 2>&1 || sudo useradd -m -s /bin/sh "$LAB"
LH=$(getent passwd "$LAB" | cut -d: -f6)
sudo -u "$LAB" mkdir -p "$LH/gate/home" "$LH/store"
lab() {
	sudo -u "$LAB" --preserve-env=PATH,TMPDIR \
		env HOME="$LH/gate/home" PODVM_STORE="$LH/store" "$@"
}
say "lab       $(lab id)"
lab sh -c 'ulimit -a' 2>&1 | sed 's/^/    limit: /' >>"$OUT"
lab timeout 300 "$P" pull -q "$IMG" >/dev/null 2>&1

n=2
for mem in 4G 1G; do
	lab timeout 600 "$P" run --verbose --rm --podvm-tier=machine --podvm-mem "$mem" "$IMG" true \
		>/dev/null 2>"$OUT.$n.err"
	rc=$?
	say "clause $n  rc $rc (user $LAB through sudo, --podvm-mem $mem)"
	[ "$rc" -eq 0 ] || { bad "clause $n"; keep "$OUT.$n.err"; kernel; }
	n=$((n + 1))
done

# Clause 4: nothing of a guest stays.
if pgrep -a qemu-system >/dev/null; then
	bad "clause 4: $(pgrep -a qemu-system | tr '\n' ' ')"
	sudo pkill -9 qemu-system || true
else
	say "clause 4  no emulator stays"
fi
rm -f "$OUT".*.err
[ "$fail" -eq 0 ] && say "PASS" || say "FAIL"
exit "$fail"

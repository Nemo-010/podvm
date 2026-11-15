#!/bin/sh
# Question: does `podvm lab run PROFILE -- CMD` make each hostile host class
# of the table with no root, and does `podvm lab check` see each part?
#
# TODO/net.md T-1916 (PodUtils/podrunner#180, #182). Since podcrates
# `v0.54.0` the profiles and the labs are `pod-lab` (podcrates@T-692): the
# lab sets each part, checks the profile from inside, and only then runs
# CMD; `check` measures this host. Run it as a user with no root, on a host
# where an unprivileged user can open a user namespace.
#
# Pinned inputs:
#   podvm   PODVM_462_BIN (default the release build of the workspace)
#   unshare, cp, sh of the host
#
# Clauses:
#   1. `lab run` of the six profiles that podrunner asked for, as one list
#      (nested labs, the profiles of a tmpfs outermost), runs `true` and
#      exits 0;
#   2. `lab check` of the same list on this host, outside a lab, exits 1
#      with at least 3 parts that fail (the control: the host allows what
#      the profiles refuse);
#   3. under userns-off, `unshare -Ur true` fails;
#   4. under noexec-tmp, a copy of `true` in /tmp gives exit 126;
#   5. under inherited-fds, descriptors 3 to 9 are open in CMD;
#   6. an unknown profile exits 125, and a missing CMD exits 127.

set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN="${PODVM_462_BIN:-$ROOT/target/x86_64-unknown-linux-musl/release/podvm}"
OUT="$ROOT/experiments/results/lab-profiles.txt"
ALL=bind-denied,getsockname-denied,userns-off,small-tmp,noexec-tmp,inherited-fds
fail=0
: >"$OUT"
say() { printf '%s\n' "$*" | tee -a "$OUT"; }
clause() { if [ "$2" = ok ]; then say "clause $1 PASS"; else say "clause $1 FAIL: $3"; fail=1; fi; }
say "date      $(date -u +%Y-%m-%dT%H:%M:%SZ)"
say "podvm     $("$BIN" version 2>/dev/null)"
say "uid       $(id -u)"

out=$("$BIN" lab run "$ALL" -- true 2>&1); rc=$?
[ -n "$out" ] && say "$out"
[ "$rc" -eq 0 ] && clause 1 ok || clause 1 no "rc $rc"

out=$("$BIN" lab check "$ALL" 2>&1); rc=$?
n=$(printf '%s\n' "$out" | grep -c "^fails")
[ "$rc" -eq 1 ] && [ "$n" -ge 3 ] && clause 2 ok || clause 2 no "rc $rc, $n parts that fail"

"$BIN" lab run userns-off -- unshare -Ur true 2>/dev/null; rc=$?
[ "$rc" -ne 0 ] && clause 3 ok || clause 3 no "rc $rc"

"$BIN" lab run noexec-tmp -- sh -c 'cp /bin/true /tmp/t && /tmp/t' 2>/dev/null; rc=$?
[ "$rc" -eq 126 ] && clause 4 ok || clause 4 no "rc $rc"

fds=$("$BIN" lab run inherited-fds -- sh -c 'n=3; while [ $n -le 9 ]; do [ -e /proc/self/fd/$n ] || echo $n; n=$((n+1)); done')
[ -z "$fds" ] && clause 5 ok || clause 5 no "closed: $fds"

"$BIN" lab run nope -- true 2>/dev/null; a=$?
"$BIN" lab run bind-denied -- /no/such/cmd 2>/dev/null; b=$?
[ "$a" -eq 125 ] && [ "$b" -eq 127 ] && clause 6 ok || clause 6 no "unknown $a, missing $b"

[ "$fail" -eq 0 ] && say "PASS" || say "FAIL"
exit "$fail"

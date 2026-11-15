#!/bin/sh
# Question: does `tar -x` on the userland rung extract a symbolic link
# that comes before its target, and a dangling one, as on the host?
#
# TODO/interpose.md T-1915 (PodUtils/podrunner#178). GNU tar sets the time
# and the mode of an extracted link with `utimensat` and `fchmodat` and
# AT_SYMLINK_NOFOLLOW; the interposer followed the link, and the call
# failed with ENOENT (tar exit 2).
#
# Pinned inputs:
#   podvm   PODVM_461_BIN (default the release build of the workspace)
#   image   mirror.gcr.io/library/debian:bookworm-slim (GNU tar)
#   rung    the userland loader family: PODVM_USERNS=0 and a -v volume
#
# Clauses:
#   1. link first, target later: exit 0, the link is a link;
#   2. target first, link later: exit 0;
#   3. a dangling link only: exit 0, the link is a link;
#   4. a hard link first: exit 0.

set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN="${PODVM_461_BIN:-$ROOT/target/x86_64-unknown-linux-musl/release/podvm}"
OUT="$ROOT/experiments/results/tar-link-order.txt"
IMG=mirror.gcr.io/library/debian:bookworm-slim
W=$(mktemp -d "${TMPDIR:-/tmp}/exp461.XXXXXX")
export PODVM_USERNS=0
fail=0
: >"$OUT"
say() { printf '%s\n' "$*" | tee -a "$OUT"; }
(
	mkdir -p "$W/src" && cd "$W/src" && echo x >target && ln -s target link && ln -s nowhere dang &&
		tar czf "$W/linkfirst.tgz" link target && tar czf "$W/targetfirst.tgz" target link &&
		tar czf "$W/dangling.tgz" dang && ln target hard && tar czf "$W/hardfirst.tgz" hard target
) || { echo "COULD NOT RUN: the tars" >&2; exit 2; }
say "date      $(date -u +%Y-%m-%dT%H:%M:%SZ)"
say "podvm     $("$BIN" version 2>/dev/null)"
timeout 300 "$BIN" pull -q "$IMG" >/dev/null 2>&1
n=0
for t in linkfirst targetfirst dangling hardfirst; do
	n=$((n + 1))
	got=$(timeout 120 "$BIN" run --rm -v "$W:/in" "$IMG" sh -c \
		"mkdir -p /tmp/x && tar xzf /in/$t.tgz -C /tmp/x; echo rc=\$?; find /tmp/x -type l | wc -l" 2>&1 | tr '\n' ' ')
	say "$t  $got"
	case "$got" in *rc=0*) ;; *) say "FAIL: clause $n"; fail=1 ;; esac
done
rm -rf "$W"
[ "$fail" -eq 0 ] && say "PASS" || say "FAIL"
exit "$fail"

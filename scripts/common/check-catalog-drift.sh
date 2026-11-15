#!/bin/sh
# TODO/machine.md T-1876: send HEAD to each pinned URL of the Windows
# catalog (crates/podvm-windows/src/catalog.rs) and compare the size that
# the server gives with the pin. It reads only and never changes a pin.
#
#   sh scripts/common/check-catalog-drift.sh          the live catalog; exit 1 on drift
#   sh scripts/common/check-catalog-drift.sh --plant  a planted pin; must exit 1
#
# A URL that answers no Content-Length is a drift too: a pin that cannot
# be checked is not a pin that holds.
set -u
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
CATALOG="$ROOT/crates/podvm-windows/src/catalog.rs"

pins() {
	# One "URL SIZE" line for each entry: the url line, then its size line.
	awk '
		/^[[:space:]]*url: "/ { u = $0; sub(/^[^"]*"/, "", u); sub(/".*$/, "", u) }
		/^[[:space:]]*size: [0-9_]+,/ && u != "" { s = $2; gsub(/[_,]/, "", s); print u, s; u = "" }
	' "$CATALOG"
}

if [ "${1:-}" = "--plant" ]; then
	# The FreeDOS pin with a size one byte off.
	list=$(pins | awk 'NR == 1 { print $1, $2 + 1 }')
else
	list=$(pins)
fi

[ -n "$list" ] || { echo "FAIL: no pin read from $CATALOG"; exit 1; }
fail=0
n=0
echo "$list" | {
	while read -r url size; do
		n=$((n + 1))
		got=$(curl -qsSIL --max-time 60 "$url" 2>/dev/null |
			tr -d '\r' | awk 'tolower($1) == "content-length:" { v = $2 } END { print v }')
		if [ "$got" = "$size" ]; then
			echo "ok    $size  $url"
		else
			echo "DRIFT pin $size, server ${got:--}  $url"
			fail=1
		fi
	done
	echo "$n pin(s) checked"
	exit "$fail"
}

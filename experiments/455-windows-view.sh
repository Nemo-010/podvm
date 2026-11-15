#!/bin/sh
# Question: does `podvm windows view NAME` give the screen of a running
# machine to a WebSocket client on the loopback, and to no client on
# another address of the host?
#
# TODO/machine.md T-1899. The machine is `run -d windows:dos` under TCG
# (KVM where /dev/kvm opens); its keeper starts QEMU with `-vnc unix:`.
# The client is a WebSocket of the experiment (RFC 6455 by hand, in
# python3 -I with no module of the network): it reads the RFB greeting.
#
# Pinned inputs:
#   podvm   PODVM_455_BIN (default the release build of the workspace)
#   store   a fresh HOME and PODVM_STORE under .work/exp455
#
# Clauses:
#   1. `windows view m1 --port P` prints http://127.0.0.1:P/, and GET /
#      answers 200 with the page;
#   2. a WebSocket client of /websockify gets 101 and a first frame that
#      starts with `RFB 003.008`;
#   3. a client on another address of the host gets no connection;
#   4. `stop m1` ends the viewer, and no emulator stays;
#   5. the page imports the noVNC client of the store, and GET
#      /novnc/core/rfb.js answers 200 with its module.

set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN="${PODVM_455_BIN:-$ROOT/target/x86_64-unknown-linux-musl/release/podvm}"
WORK="$ROOT/.work/exp455"
OUT="$ROOT/experiments/results/windows-view.txt"
rm -rf "$WORK"
mkdir -p "$WORK/home"
export HOME="$WORK/home" PODVM_STORE="$WORK/store"
fail=0
: >"$OUT"
say() { printf '%s\n' "$*" | tee -a "$OUT"; }
bad() { say "FAIL: $*"; fail=1; }

say "date      $(date -u +%Y-%m-%dT%H:%M:%SZ)"
say "podvm     $("$BIN" version 2>/dev/null)"
timeout 600 "$BIN" pull windows:dos >/dev/null 2>&1
timeout 120 "$BIN" run -d --name m1 windows:dos >/dev/null 2>"$WORK/run.err" || { bad "run -d"; cat "$WORK/run.err" | tee -a "$OUT"; }
sleep 3
PORT=$(python3 -I -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')
"$BIN" windows view m1 --port "$PORT" >"$WORK/view.out" 2>"$WORK/view.err" &
VIEW=$!
sleep 2
say "view      $(cat "$WORK/view.out")"
grep -qx "http://127.0.0.1:$PORT/" "$WORK/view.out" || bad "clause 1: the URL"

cat >"$WORK/ws.py" <<'EOF'
import os, socket, sys
host, port, mode = sys.argv[1], int(sys.argv[2]), sys.argv[3]
s = socket.create_connection((host, port), timeout=10)
if mode in ("page", "rfb"):
    path = b"/" if mode == "page" else b"/novnc/core/rfb.js"
    s.sendall(b"GET " + path + b" HTTP/1.1\r\nHost: x\r\n\r\n")
    data = b""
    while True:
        b = s.recv(65536)
        if not b:
            break
        data += b
    head, _, body = data.partition(b"\r\n\r\n")
    print(head.split(b"\r\n")[0].decode(), len(body), "rfb.js" in body.decode(errors="replace"), "export default class RFB" in body.decode(errors="replace"))
    sys.exit(0)
key = "dGhlIHNhbXBsZSBub25jZQ=="
s.sendall(("GET /websockify HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\n"
           "Connection: Upgrade\r\nSec-WebSocket-Key: %s\r\n"
           "Sec-WebSocket-Version: 13\r\nSec-WebSocket-Protocol: binary\r\n\r\n" % key).encode())
head = b""
while b"\r\n\r\n" not in head:
    head += s.recv(1)
status = head.split(b"\r\n")[0].decode()
accept = [l for l in head.split(b"\r\n") if l.lower().startswith(b"sec-websocket-accept")]
h = s.recv(2)
n = h[1] & 0x7f
if n == 126:
    n = int.from_bytes(s.recv(2), "big")
payload = b""
while len(payload) < min(n, 12):
    payload += s.recv(n - len(payload))
print(status, accept[0].decode() if accept else "-", repr(payload[:12].decode(errors="replace")))
EOF
page=$(python3 -I "$WORK/ws.py" 127.0.0.1 "$PORT" page 2>&1)
say "page      $page"
printf '%s' "$page" | grep -q "200 OK" || bad "clause 1: GET /"
printf '%s' "$page" | grep -q "True" || bad "clause 5: the page does not import rfb.js"
rfb=$(python3 -I "$WORK/ws.py" 127.0.0.1 "$PORT" rfb 2>&1)
say "rfb.js    $rfb"
printf '%s' "$rfb" | grep -q "200 OK .* True$" || bad "clause 5: /novnc/core/rfb.js"
ws=$(python3 -I "$WORK/ws.py" 127.0.0.1 "$PORT" ws 2>&1)
say "websocket $ws"
printf '%s' "$ws" | grep -q "101 Switching Protocols" && printf '%s' "$ws" | grep -q "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=" && printf '%s' "$ws" | grep -q "RFB 003.008" || bad "clause 2"

OTHER=$(ip -4 -o addr show scope global 2>/dev/null | awk '{print $4}' | cut -d/ -f1 | head -1)
if [ -n "$OTHER" ]; then
	far=$(python3 -I "$WORK/ws.py" "$OTHER" "$PORT" page 2>&1 | tail -1)
	say "other     $OTHER: $far"
	printf '%s' "$far" | grep -qi "refused\|timed out" || bad "clause 3: $OTHER answered"
else
	say "other     no other address on this host: clause 3 not run"
fi

timeout 60 "$BIN" stop m1 >/dev/null 2>&1
sleep 3
if kill -0 "$VIEW" 2>/dev/null; then
	bad "clause 4: the viewer outlived the machine"
	kill "$VIEW"
fi
say "ended     $(tail -1 "$WORK/view.err")"
"$BIN" rm m1 >/dev/null 2>&1
if pgrep -f "qemu-system-x86_64.*$WORK" >/dev/null; then
	bad "clause 4: an emulator stayed"
	pkill -9 -f "qemu-system-x86_64.*$WORK"
fi
rm -rf "$WORK"
[ "$fail" -eq 0 ] && say "PASS" || say "FAIL"
exit "$fail"

#!/usr/bin/env python3
# SPDX-License-Identifier: 0BSD
"""The pins of an installer of PodUtils (docs/install.md).

  pins.py sums DIR FILE...
      write DIR/SHA256SUMS: the SHA-256 of each FILE of DIR, sorted
  pins.py stamp INSTALLER --version X.Y.Z --assets DIR --out OUT
      write OUT: INSTALLER with the core (scripts/install/core.sh and
      scripts/install/core-net.sh) inline in its block of the core, and the
      block of pins for each file of DIR/SHA256SUMS (its version, its
      SHA-256 and its size); a file that does not agree with its line of
      SHA256SUMS fails
  pins.py check INSTALLER
      exit 1 when the copy of the tree holds pins, or has no block of
      pins, or its block of the core does not include each file of the core
  pins.py bundle OUT.tar.gz NAME FILE...
      write the offline bundle: each FILE in the directory NAME, with
      mode 0755 and the time SOURCE_DATE_EPOCH (else 0); the same input
      gives the same bytes

Exit 0 when done, 1 at a fault, 2 at a usage error.
"""

import gzip
import hashlib
import io
import os
import re
import sys
import tarfile
from pathlib import Path

BEGIN = "# BEGIN PINS (scripts/install/pins.py writes this block; do not edit it)"
END = "# END PINS"
CORE_BEGIN = "# BEGIN CORE (scripts/install/pins.py puts scripts/install/core.sh here)"
CORE_END = "# END CORE"
# The files of the core, in the order of their include.
CORE = [Path(__file__).resolve().parent / n for n in ("core.sh", "core-net.sh")]
VERSION = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$")
TARGET = re.compile(r"-((?:x86_64|aarch64|armv7|arm|i686|riscv64gc|powerpc64le|loongarch64)-[a-z0-9_-]+?)(?:\.exe)?$")
NAME = re.compile(r"^[A-Za-z0-9._+-]+$")


def fault(msg: str) -> int:
    print(f"pins: {msg}", file=sys.stderr)
    return 1


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def key(asset: str) -> str:
    return re.sub(r"[^A-Za-z0-9]", "_", asset)


def read_sums(path: Path) -> list[tuple[str, str]]:
    out = []
    for n, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        parts = line.split()
        if len(parts) != 2 or not re.fullmatch(r"[0-9a-f]{64}", parts[0]):
            raise ValueError(f"{path}:{n}: not a line 'SHA256  NAME'")
        name = parts[1].lstrip("*")
        if not NAME.match(name):
            raise ValueError(f"{path}:{n}: {name!r} is not a file name")
        out.append((parts[0], name))
    if not out:
        raise ValueError(f"{path}: no line")
    return out


def split(text: str, begin: str, end: str, name: str) -> tuple[str, str, str]:
    start, stop = text.find(begin + "\n"), text.find(end + "\n")
    if start < 0 or stop < start:
        raise ValueError(f"{name}: no block {begin!r} ... {end!r}")
    stop += len(end) + 1
    return text[:start], text[start:stop], text[stop:]


def block(version: str, assets: Path) -> str:
    lines = [BEGIN, f'INST_VERSION="{version}"']
    sums = read_sums(assets / "SHA256SUMS")
    targets = []
    pins = []
    for want, name in sums:
        f = assets / name
        if not f.is_file():
            raise ValueError(f"{f}: missing: SHA256SUMS names it")
        have = sha256(f)
        if have != want:
            raise ValueError(f"{name}: the SHA-256 is {have}, not {want} of SHA256SUMS")
        m = TARGET.search(name)
        if m and m.group(1) not in targets:
            targets.append(m.group(1))
        # Read by name through eval (inst_pin, inst_size): shellcheck cannot see it.
        pins.append("# shellcheck disable=SC2034 # read by name through eval")
        pins.append(f'INST_SUM_{key(name)}="{want}"')
        pins.append("# shellcheck disable=SC2034 # read by name through eval")
        pins.append(f'INST_SIZE_{key(name)}="{f.stat().st_size}"')
    lines.append(f'INST_TARGETS="{" ".join(targets)}"')
    return "\n".join(lines + pins + [END]) + "\n"


def include(path: Path) -> str:
    return f'. "$(dirname "$0")/{path.name}"'


def inline_core() -> str:
    bodies = []
    for path in CORE:
        text = path.read_text(encoding="utf-8")
        bodies.append((text.split("\n", 1)[1] if text.startswith("#!") else text).rstrip())
    return f"{CORE_BEGIN}\n" + "\n".join(bodies) + f"\n{CORE_END}\n"


def stamp(args: list[str]) -> int:
    opts = {"--version": "", "--assets": "", "--out": ""}
    rest = []
    while args:
        a = args.pop(0)
        if a in opts and args:
            opts[a] = args.pop(0)
        else:
            rest.append(a)
    if len(rest) != 1 or not all(opts.values()):
        return usage()
    if not VERSION.match(opts["--version"]):
        return fault(f"--version {opts['--version']!r} is not X.Y.Z")
    src = Path(rest[0])
    try:
        text = src.read_text(encoding="utf-8")
        head, _, tail = split(text, BEGIN, END, str(src))
        text = head + block(opts["--version"], Path(opts["--assets"])) + tail
        head, _, tail = split(text, CORE_BEGIN, CORE_END, str(src))
        text = head + inline_core() + tail
    except (OSError, ValueError) as e:
        return fault(str(e))
    out = Path(opts["--out"])
    # LF on each host: sh reads a CR as a part of a word (Python on Windows).
    out.write_text(text, encoding="utf-8", newline="\n")
    out.chmod(0o755)
    print(f"pins: {out}: the core and the pins of {opts['--version']} are written")
    return 0


def check(args: list[str]) -> int:
    if len(args) != 1:
        return usage()
    try:
        text = Path(args[0]).read_text(encoding="utf-8")
        _, pins, _ = split(text, BEGIN, END, args[0])
        _, core, _ = split(text, CORE_BEGIN, CORE_END, args[0])
    except (OSError, ValueError) as e:
        return fault(str(e))
    want = f'{BEGIN}\nINST_VERSION=""\nINST_TARGETS=""\n{END}\n'
    if pins != want:
        return fault(f"{args[0]}: the copy of the tree holds pins: only a release writes them")
    for path in CORE:
        if include(path) not in core:
            return fault(f"{args[0]}: the block of the core does not include {path.name}")
    print(f"pins: {args[0]}: no pins, and the core is included")
    return 0


def sums(args: list[str]) -> int:
    if len(args) < 2:
        return usage()
    d = Path(args[0])
    lines = []
    for name in sorted(args[1:]):
        if not NAME.match(name) or not (d / name).is_file():
            return fault(f"{d / name}: not a file of {d}")
        lines.append(f"{sha256(d / name)}  {name}\n")
    (d / "SHA256SUMS").write_text("".join(lines), encoding="utf-8", newline="\n")
    print(f"pins: {d / 'SHA256SUMS'}: {len(lines)} files")
    return 0


def bundle(args: list[str]) -> int:
    if len(args) < 3 or not NAME.match(args[1]):
        return usage()
    out, name, files = Path(args[0]), args[1], [Path(f) for f in args[2:]]
    mtime = int(os.environ.get("SOURCE_DATE_EPOCH", "0") or 0)
    raw = io.BytesIO()
    with tarfile.open(fileobj=raw, mode="w", format=tarfile.USTAR_FORMAT) as t:
        d = tarfile.TarInfo(name)
        d.type, d.mode, d.mtime = tarfile.DIRTYPE, 0o755, mtime
        t.addfile(d)
        for f in sorted(files, key=lambda p: p.name):
            data = f.read_bytes()
            i = tarfile.TarInfo(f"{name}/{f.name}")
            i.size, i.mode, i.mtime = len(data), 0o755, mtime
            t.addfile(i, io.BytesIO(data))
    with out.open("wb") as f, gzip.GzipFile(fileobj=f, mode="wb", mtime=mtime, filename="") as g:
        g.write(raw.getvalue())
    print(f"pins: {out}: {len(files)} files in {name}/")
    return 0


def usage() -> int:
    print(__doc__.strip().split("\n\n")[1], file=sys.stderr)
    return 2


def main(argv: list[str]) -> int:
    commands = {"stamp": stamp, "check": check, "sums": sums, "bundle": bundle}
    if not argv or argv[0] not in commands:
        return usage()
    return commands[argv[0]](argv[1:])


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

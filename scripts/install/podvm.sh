#!/bin/sh
# SPDX-License-Identifier: 0BSD
# The installer of podvm (TODO/cli.md T-1918, the owner's ruling
# "Installer"), on the installer core of podcrates (scripts/install/core.sh,
# podcrates `v0.58.0`). A release publishes it as podvm-install.sh, with the
# core inline and the pins of the release (scripts/install/pins.py stamp).
# This copy of the tree has no pins: it refuses to install.
#
#   sh podvm-install.sh [--prefix DIR] [--dry-run] [--offline FILE] [--add-path FILE]
INST_NAME=podvm
INST_ENV=PODVM
INST_REPO=PodUtils/podvm

# BEGIN PINS (scripts/install/pins.py writes this block; do not edit it)
INST_VERSION=""
INST_TARGETS=""
# END PINS

# BEGIN CORE (scripts/install/pins.py puts scripts/install/core.sh here)
# shellcheck source=scripts/install/core.sh
. "$(dirname "$0")/core.sh"
# END CORE

# The assets of a release name the machine, not the triple: podvm-x86_64.
inst_files() {
    case $1 in
        x86_64-unknown-linux-musl) inst_a=x86_64 ;;
        aarch64-unknown-linux-musl) inst_a=aarch64 ;;
        armv7-unknown-linux-musleabihf) inst_a=armv7 ;;
        i686-unknown-linux-musl) inst_a=i686 ;;
        riscv64gc-unknown-linux-musl) inst_a=riscv64gc ;;
        powerpc64le-unknown-linux-musl) inst_a=powerpc64le ;;
        loongarch64-unknown-linux-musl) inst_a=loongarch64 ;;
        *) return 0 ;;
    esac
    printf '%s %s\n' "podvm" "podvm-$inst_a"
}

# The names docker and podman: podvm reads argv[0]. The installer makes no
# link, as a link would hide a docker or a podman of the host.
inst_setup() {
    inst_say "podvm answers to the names docker and podman too; to use them, make a link:"
    inst_say "  ln -s $1/podvm $1/docker"
    return 0
}

# The self-check: the version and the probe of this host, with no network.
inst_selfcheck() {
    if ! "$1/podvm" version --verbose > /dev/null 2>&1; then
        "$1/podvm" version --verbose 2>&1 | tail -n 5 >&2
        return 1
    fi
    inst_say "self-check: podvm version: ok"
    if inst_p=$("$1/podvm" probe 2> /dev/null); then
        inst_say "self-check: podvm probe: $inst_p"
    else
        inst_say "self-check: podvm probe could not select a rung on this host; podvm doctor says why"
    fi
    return 0
}

inst_main "$@"

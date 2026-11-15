# Handoff

The state on 2026-11-15, later. `0.2.0-beta.21` is released (issue 392:
gate, arm, nightly trial and release green on `e4dd9168`). It carries the
store copy of QEMU (T-1875), the detached Windows and DOS machines with
`ps`, `stop`, `start`, `rm`, `logs -f`, `port` and `-p` (T-1870, T-1869),
`windows screenshot` and `windows view` (T-1899), and a `:ro` volume on the
native-static rung (T-1901, PodUtils/podrunner#111). `main` adds the
arm64 guest of the machine tier (T-1890), the source-pattern ratchet of
the gate (T-1855), the machine case of the report test (T-1856) and the
evaluation days in the Windows doctor (T-1871). Request 388 runs the first
Windows install under KVM.
A fresh
session with no memory reads this file first, then `AGENTS.md`,
`TODO/PROGRESS.md`, `TODO/RESUME.md` and `docs/decisions.md`. Remove this
file when each step below is done.

## Where things are

* The repository is PodUtils/podvm on `main`. No work lives in a local
  worktree or a branch. The branch `archive/podbox-worktree` is an
  archive; never merge or delete it.
* **A cloud session cannot start a workflow run** (`TODO/PROGRESS.md`,
  State). Each run is an issue titled `[gh action] requested` in the form
  of `AGENTS.md`, section 9, item 5. While a request that checks the head
  of `main` is open, push nothing to `main`. The owner's local agent ran
  issue 287 in about 50 minutes, and it reported the run ids.
* The record is in the format of `pod-todo` (T-1672, call C17).
  `podvm-count --set T-NNNN STATUS` writes the status line and the row;
  a done entry needs `## Done` with a date, and a partial entry also puts
  its state under `## Done`. A foreign entry id carries its repository
  (`PodUtils/podrunner@T-096`), or check 35 refuses it. A path of another
  repository in an entry is named in words, not in backticks.
* incus runs on the cloud host (`docs/decisions.md`, "Missing tools"). The
  host has no `/dev/kvm`; the machine tier runs TCG there.

## The QA pass

Experiment 443 (`experiments/443-qa-surface.sh`) drives the CLI on the
namespace rung and on the chroot-denied fixture (`ul`). The first pass
gave 518 PASS and 64 FAIL, and T-1753 to T-1771; the reruns found T-1772
to T-1775. One full run on the published `v0.2.0-beta.14` (2026-11-14) gives 583
PASS, 0 FAIL and 13 INFO, with no merge of earlier runs.

## Open COORD issues

* COORD-041 (#260): `pod-netlab` for T-1662.
* COORD-112 (#366): the answer to PodUtils/podrunner#102 (T-1889, T-1890).
* COORD-117 (#374): podvm took podcrates `v0.48.0`.
* COORD-120 (#380): podvm took `v0.49.0`; a Validation OS run under KVM
  timed out on one cold boot of two (T-1864); it did not recur.
* COORD-122 (#384): the hook of the run ladder for T-1786.
* COORD-123 (#386): a FAT writer with directories for `-v` of a Windows
  guest (T-1872).
* COORD-124 (#389): `pod-services` lists `api.ghcr.pkgforge.dev` (T-1875);
  until then the host is in `WAITING` of `crates/podvm/src/hosts_tests.rs`.
* COORD-125 (#390): a leg of `pod-hostprobe` for a delegated cgroup2
  subtree (T-1747).
* COORD-127 (#393): a client of the QEMU Guest Agent (T-1868).
* COORD-128 (#394): the aarch64 netboot pins in `pod-services` (T-1890).

Each other COORD issue is closed; `TODO/issues.md` gives the state of
each one.

## Next steps, in order

1. Start the session (`AGENTS.md`, section 8).
2. The next release follows a batch of fixes, in the form of issue 392.
3. The Windows proof is on KVM. Request 388 runs `kvm.yml` with the new
   job `install` (experiment 452: `pull windows:2025` and `run
   windows:2025 ver`). Read its transcript, record the first install
   time in T-1867, then add `windows:11` to the job. A failure there is
   most likely the "Press any key" of the ISO, the disk order of Setup
   (the answer file wipes disk 0) or the image index of the Server ISO.
4. A tool of the record (`podvm-gate`, `podvm-count`) finds the repository
   from its own path: in a worktree, build it in the worktree's own
   `target/`, or it reads and writes the main checkout. Commit no
   symbolic link: issue 305 stopped on one.
5. Before an entry is set done, run `cargo test -p podvm`. Do not edit
   `TODO/` while `cargo test --workspace` runs, and do not edit
   `experiments/443-qa-surface.sh` while it runs.

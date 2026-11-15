# Handoff

The state on 2026-11-15. `0.2.0-beta.8` (issue 302) is the complete
release for the beta testers (`docs/decisions.md`, "Complete release
first"); beta.9 to beta.18 serve podrunner, podswarm and the fixes of the
QA reports. `main` holds the version `0.2.0-beta.19` and its changelog
section, with podcrates `v0.49.0`: `container prune`, the detached driver
of the native-static rung (PodUtils/podrunner#100, case 2), `save` of an
index digest (PodUtils/podrunner#104), `--flag=value` for each flag, and
the end of a detached report. The release request of beta.19 (issue 378)
stopped at a test fixed in `88019eed`; the next request follows.
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
* COORD-118 (#375) and COORD-119 (#376): the answers to
  PodUtils/podrunner#104 and #100; close each when beta.19 is out.

Each other COORD issue is closed; `TODO/issues.md` gives the state of
each one.

## Next steps, in order

1. Start the session (`AGENTS.md`, section 8).
2. Release beta.19: a `[gh action] requested` issue for the gate,
   `arm.yml`, the nightly trial and the release, at the head of `main`;
   push nothing while it is open. Then close issues 375 and 376 with the
   tag.
3. The Windows proof is on KVM: run request 377 runs `kvm.yml`
   (experiments 392 and 450) for T-1864 and T-1894. Under TCG on the
   cloud host the console of Validation OS does not draw in 15 minutes
   (T-1864).
4. A tool of the record (`podvm-gate`, `podvm-count`) finds the repository
   from its own path: in a worktree, build it in the worktree's own
   `target/`, or it reads and writes the main checkout. Commit no
   symbolic link: issue 305 stopped on one.
5. Before an entry is set done, run `cargo test -p podvm`. Do not edit
   `TODO/` while `cargo test --workspace` runs, and do not edit
   `experiments/443-qa-surface.sh` while it runs.

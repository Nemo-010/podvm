# Handoff

The state on 2026-11-15, night. `0.2.0-beta.22` is released (request 410,
release run 43708756057, with attestations: T-1905 done). `0.2.0-beta.23`
waits on request 412 (`86574c96`): it fixes defects of the user-namespace
entry of beta.22 (T-1909: a tree left by each run, placeholder `/dev`
nodes, noise of the first extraction, a race of two fills across PID
namespaces), adds `exec` and `cp` into such a run (T-1910) and `exec` with
the name of `--hostname` (T-1908), and takes podcrates `v0.51.0` (the
arm64 guest fetches its pinned kernel, T-1890). The local commits after
`86574c96` wait in `.work/HOLD` until request 412 ran. Request 411 runs
the Windows install in three legs (run 43709734103); its store leg failed
in the TCG refusal step, fixed in `e69bd045`.
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
* COORD-131 (#404): the answer to PodUtils/podrunner#115 (`--hostname`).
* COORD-132 (#406): the user namespace in pod-enter, for `run -d` and
  `create` (T-1909 serves the foreground `run` in podvm).
* COORD-133 (#407): the `kvm` leg of `pod-hostprobe` checks the
  architecture of the guest (T-1907).

Each other COORD issue is closed; `TODO/issues.md` gives the state of
each one.

## Next steps, in order

1. Start the session (`AGENTS.md`, section 8).
2. Request 412 releases beta.23. Push nothing to `main` until it ran;
   then push the held commits, record the run ids, and close 412.
3. Run 43709734103 (request 411): read the screenshots of the legs
   `ubuntu-qemu` and `no-hyperv` (`gh api
   repos/PodUtils/podvm/actions/artifacts/ID/zip`). The leg that passes
   names the cause of the stall at "56% complete"; record it in T-1867.
   The store leg needs a new run (its step was fixed after the dispatch).
4. A tool of the record (`podvm-gate`, `podvm-count`) finds the repository
   from its own path: in a worktree, build it in the worktree's own
   `target/`, or it reads and writes the main checkout. Commit no
   symbolic link: issue 305 stopped on one.
5. Before an entry is set done, run `cargo test -p podvm`. Do not edit
   `TODO/` while `cargo test --workspace` runs, and do not edit
   `experiments/443-qa-surface.sh` while it runs.

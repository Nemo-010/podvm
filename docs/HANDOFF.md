# Handoff

The state on 2026-11-16, 04:00 (UTC). `main` is at the commit that adds
this file, after `ecb37294` (T-1920 done). `0.2.0-beta.24` is the last
release. A fresh session with no memory reads this file first, then
`AGENTS.md`, `TODO/PROGRESS.md` and `docs/decisions.md`. Remove this file
when each step below is done.

## Where things are

* The repository is PodUtils/podvm on `main`. No work lives in a local
  worktree, a stash or a branch. The branch `archive/podbox-worktree` is
  an archive; never merge or delete it.
* **A cloud session cannot start a workflow run.** Each run is an issue
  titled `[gh action] requested` in the form of `AGENTS.md`, section 9,
  item 5. While a request that checks the head of `main` is open, push
  nothing to `main`. No such request is open now: #441 ran (see below).
* The record is in the format of `pod-todo`. `podvm-count --set T-NNNN
  STATUS` writes the status line and the row; edit the count line of
  `TODO/PROGRESS.md` by hand to the numbers that it prints. The first
  paragraph of `## Done` of a done entry starts with `**Done`.
* podcrates is taken up to `v0.59.0` (`Plan::volumes` of pod-enter).
* incus runs on the cloud host. The host has no `/dev/kvm`.
* Integration tests run as uid 1000: `setpriv --reuid 1000 --regid 1000
  --clear-groups env HOME=$T TMPDIR=$T PATH=$PATH
  CARGO_MANIFEST_DIR=$PWD/crates/podvm TESTBIN`, with `T` a new directory
  under `/var/tmp` with mode 777. Root hides defects of the user
  namespace.

## Done in the last session

* T-1920 done (`ecb37294`): `run -d` and `create` bind each volume through
  pod-enter's `Plan::volumes` and keep the namespace rung
  (`crates/podvm/src/run/binds.rs`, `PODVM_BINDS` in the record, read by
  `crates/podvm-supervise/src/launcher.rs`). COORD-148 (#440) is closed.
* T-1747 done (cgroup2 limits), T-1912 done, T-1907 done, the installer
  (T-1918) and `podvm lab run --allow-connect` are on `main`.

## Open defect: `tmpfs_exec` fails on `main` (first task)

The integration test `crates/podvm/tests/tmpfs_exec.rs` fails as uid 1000
on `ac370178` and on `ecb37294`. It passed on `aa281ba7` (the take of
`v0.59.0`) in the full suite, so the cause is likely in `aa281ba7` (the
podcrates `v0.59.0` update), in `21a607fd`, or in the state of the host.
Measured: `-` (bisect not done).

Reproduce as uid 1000: `echo run | podvm run -i --name ty
busybox:1.37.0-musl tee /tmp/x`, then `podvm exec ty cat /tmp/x` gives
"chroot into the rootfs failed with EPERM", exit 125. The run records
`"Rung":"namespace"` (foreground `run` enters podvm's own user namespace,
T-1909). `exec` of the stopped run is not in that namespace. Its probe
gives `chroot=EPERM`, and its `--verbose` banner says `mode=chroot` and
"this machine would permit `namespace`", but `decide_entry` gives no
userland family and the chroot sequence runs. Read
`crates/podvm/src/exec/enter.rs` (lines 100 to 170, 410 to 440) and
`crates/podvm/src/lifecycle/entry.rs` (`chroot_ok_for`,
`decide_entry_prefer`). The fix probably is: `exec` of a run that is not
running, on a host where `chroot` is refused but the user-namespace leg
passes, enters through a user namespace (as `run` does), or takes the
userland family. Record it as a new entry with the Source "integration
test tmpfs_exec". The test `completion_reach` is flaky (known).

## kvm run 43783602911 (request #441)

The cgroup, Linux guest and aarch64 jobs pass. The ValidationOS job fails
again (T-1350, intermittent; its log tail shows only the clean-up; read
the artifact `kvm-windows-transcript`). Both Windows install legs fail:
`ubuntu-qemu` (4 GiB) and `ubuntu-qemu-8g` (8 GiB, 4 CPUs) (T-1867). Read
their logs and the step "the logs of Setup", record the result in T-1867
and T-1350, then close #441 with the evidence. The owner's agent wrote
the result as comment 6906673035 on #441.

## Next steps, in order

1. Start the session (`AGENTS.md`, section 8), with the watcher.
2. Fix `tmpfs_exec` (above). Run the whole uid-1000 suite before the push.
3. Read run 43783602911; update T-1867 and T-1350; close #441.
4. Amend COORD-147 (#439, to podrunner) with a state line: T-1920 is done
   on `main` at `ecb37294` for `run`, `run -d` and `create`; it ships in
   the next release. Then close it when the release carries it.
5. Release `0.2.0-beta.25` (T-1920, T-1747, the installer T-1918, the
   machine-tier message for PodUtils/podrunner#193) through a
   `[gh action] requested` issue for `release.yml`. Then tell podrunner
   the tag (#439, #442) and run the installer in the labs.
6. Verify the first scheduled run of `issue-sync.yml` (06:17 UTC) and
   finish T-1919.
7. Then the work order of `TODO/PROGRESS.md` (T-1917 remainder, T-1911,
   T-1868).

## Rules learned

* A tool of the record (`podvm-gate`, `podvm-count`) finds the repository
  from its own path: in a worktree, build it in the worktree's own
  `target/`.
* Before an entry is set done, run its proof and the uid-1000 suite.
* The disk is small: remove `target/debug` and stale `deps` when it fills.

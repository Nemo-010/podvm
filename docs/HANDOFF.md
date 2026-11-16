# Handoff

The state on 2026-11-16, 08:10 (UTC). `main` is at the commit that adds
this file. `0.2.0-beta.25` is the last release (release run 43825580962
on `71099545`). A fresh session with no memory reads this file first,
then `AGENTS.md`, `TODO/PROGRESS.md` and `docs/decisions.md`. Remove this
file when each step below is done.

## Where things are

* The repository is PodUtils/podvm on `main`. No work lives in a local
  worktree, a stash or a branch. The branch `archive/podbox-worktree` is
  an archive; never merge or delete it.
* **A cloud session can start workflow runs again.** A push to `main`
  starts the gate, `secrets` and `issue-sync`; `actions_run_trigger`
  dispatches `kvm.yml`, `arm.yml`, `nightly.yml` and `release.yml`. A
  newer push cancels the gate of an older commit. The issue `[gh action]
  requested` is only the fallback.
* `issue-sync.yml` runs on each push and writes the state of each issue
  from `TODO/issues.md` and `TODO/github.txt`. A wrong row reopens or
  closes an issue on GitHub, so keep the rows right in the same commit.
* The pkgforge mirrors (the owner's ruling "Web access") answer 403
  through the egress proxy of a podvm cloud session; use the GitHub MCP
  tools then.
* podcrates is taken up to `v0.60.0` (no adoption patch for podvm; the
  build, clippy, the unit tests and the uid-1000 suite pass; COORD-152,
  #445). podvm shadows `chroot_usable` of
  pod-hostprobe in `podvm-probe` (T-1921) until COORD-150 (#443) is
  answered.
* The record is in the format of `pod-todo`. `podvm-count` writes the
  rows and the counts of `TODO/INDEX.md`; edit the count line of
  `TODO/PROGRESS.md` by hand to the numbers that it prints.
* Integration tests run as uid 1000: `setpriv --reuid 1000 --regid 1000
  --clear-groups env HOME=$T TMPDIR=$T PATH=$PATH
  CARGO_MANIFEST_DIR=$PWD/crates/podvm TESTBIN`, with `T` a new directory
  under `/var/tmp` with mode 777. Build the interposer objects first
  (`scripts/build-interpose.sh`), or each loader-family test fails. Two
  failures come from the host, not the code: `report_path_is_the_parsers`
  needs a writable working directory, and `podvm_buildstate` cannot run
  root's `rustc` as uid 1000.
* `public.ecr.aws` answered 429 to the cloud host on 2026-11-16, so
  experiment 356 runs only in `arm.yml`.

## Done in this session

* T-1921 done: `chroot_usable` read the leg in a user namespace as the
  caller's `chroot`, so `exec` of a stopped run entered into EPERM
  (`tmpfs_exec`). COORD-150 (#443) asks podcrates for the fix.
* T-1923 done: the namespace rung through a user namespace needs `chroot`
  there too (experiment 356 in `arm.yml` passes).
* T-1922 partial: the machine tier names how the emulator ended;
  experiment 463 shows that a user with no root through `sudo -u`
  passes. COORD-151 (#444) asks podrunner to run its probe with beta.25.
* Run 43783602911 read into T-1867 (Setup shows its failure dialog within
  minutes; 8 GiB is not the fix) and T-1350; #441 closed.
* Released `0.2.0-beta.25`; #439 and #442 name the tag.

## Next steps, in order

1. Start the session (`AGENTS.md`, section 8), with the watcher.
2. A new ask of a peer first: watch #444 (podrunner's stderr with
   beta.25), #439 and #442 (podrunner confirms the binds), #443
   (podcrates answers COORD-150; then take the tag, remove the shadow in
   `crates/podvm-probe/src/lib.rs`, and close T-1921's COORD).
3. Close COORD-152 (#445, the take of `v0.60.0`) when podcrates reads
   it.
4. T-1867: keep the logs of WinPE (`X:\Windows\Panther`,
   `X:\$Windows.~BT`) before the power-off of a failed install, to read
   the real error of Setup.
5. T-1919: prove that `podvm-gate` fails on a row whose state differs
   from GitHub, then set it done.
6. Then the work order of `TODO/PROGRESS.md` (T-1917 remainder, T-1911,
   T-1868).

## Rules learned

* A tool of the record (`podvm-gate`, `podvm-count`) finds the repository
  from its own path: in a worktree, build it in the worktree's own
  `target/`.
* Before an entry is set done, run its proof and the uid-1000 suite.
* Before a release, run `arm.yml` too: experiment 356 found T-1923, which
  the gate and the nightly trial do not cover.
* The sudo of a GitHub runner keeps `XDG_CONFIG_HOME` of the runner's
  user; an experiment that makes a second user removes each `XDG_*`.
* The disk is small: remove `target/debug` and stale `deps` when it fills.

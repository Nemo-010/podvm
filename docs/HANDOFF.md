# Handoff

The state on 2026-11-16, 12:45 (UTC). `main` is at the commit that adds
this file. `0.2.0-beta.27` is the last release (release run 43856299505
on `290c4de2`). A fresh session with no memory reads this file first,
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
* podcrates is taken up to `v0.63.0` (PodUtils/podcrates#211). Since
  `v0.62.0` (PodUtils/podcrates#210): its
  `chroot_usable` is fixed (COORD-150) and `Plan::userns_ids` serves
  T-1927 (COORD-154). The way out of a network lab (T-1924), the served
  ports of a lab (no entry yet) and the installer core of `v0.61.0`
  (T-1925) are not taken.
* The record is in the format of `pod-todo`. `podvm-count` writes the
  rows and the counts of `TODO/INDEX.md`; edit the count line of
  `TODO/PROGRESS.md` by hand to the numbers that it prints.
* Integration tests run as uid 1000: `setpriv --reuid 1000 --regid 1000
  --clear-groups env HOME=$T TMPDIR=$T PATH=$PATH
  CARGO_MANIFEST_DIR=$PWD/crates/podvm TESTBIN`, with `T` a new directory
  under `/var/tmp` with mode 777. Build the interposer objects first
  (`scripts/build-interpose.sh`), or each loader-family test fails. Three
  failures come from the host, not the code: `report_path_is_the_parsers`
  needs a writable working directory, and `podvm_buildstate` and
  `package_ssh` cannot run root's `rustc` and `cargo` as uid 1000.
* `public.ecr.aws` answered 429 to the cloud host on 2026-11-16, so
  experiment 356 runs only in `arm.yml`.

## Done in this session

* Released `0.2.0-beta.25` (T-1920, T-1921, T-1923), `0.2.0-beta.26`
  (T-1928, T-1926, the message of T-1922) and `0.2.0-beta.27` (T-1927).
* T-1917 partial: `podvm man --llms`, and the date of the page from
  `SOURCE_DATE_EPOCH`; the assets of `release.yml` remain.
* T-1928 (P0): a program that reused descriptor 17 got the ownership memo
  read and written on its own file (Node under npm 6 read empty files).
  The host hands `PODVM_MEMO_ID`, and the interposer checks it.
* T-1926: `run -d --hostname` keeps the name on the namespace rung.
* T-1927 done: `run -d` and `create` (beta.27) and a foreground `run`
  (podcrates `v0.63.0`, on `main`) run as the image's `USER` or
  `--user` inside the user namespace.
* T-1922 partial: experiment 463 shows that neither a user with no root
  nor a root of 300 MiB stops the machine tier; beta.26 names why podvm
  stopped waiting, the time and the initrd. COORD-158 (#451) asks
  podrunner for its run with beta.26.
* The size ceiling moved to 8 500 000 bytes (call C23, T-0910).
* Run 43783602911 read into T-1867 and T-1350; #441 closed.
* T-1919 done: `podvm-gate` names a row whose state differs from GitHub,
  and `issue-sync.yml` closes an issue whose entries are done.

## Next steps, in order

1. Start the session (`AGENTS.md`, section 8), with the watcher.
2. A new ask of a peer first. Watch #451 (podrunner's stderr with
   beta.26, T-1922), #450 (descriptor 17), #448 (`--hostname` and the
   user of the image: write the tag of T-1927 there with the next beta),
   #439, #442 and #444 (older answers to podrunner; close each when
   podrunner confirms).
3. T-1927 is done: `run -d` is in beta.27 (#448 names the tag), and the
   foreground `run` (podcrates `v0.63.0`) ships in the next beta.
4. Close COORD-152, COORD-153, COORD-156 and COORD-160 (#445, #446,
   #449, #453, the takes of `v0.60.0` to `v0.63.0`) when podcrates reads
   them.
5. T-1867: keep the logs of WinPE (`X:\Windows\Panther`,
   `X:\$Windows.~BT`) before the power-off of a failed install, to read
   the real error of Setup.
6. Then the work order of `TODO/PROGRESS.md` (T-1917 remainder, T-1911,
   T-1868, T-1924, T-1925).

## Rules learned

* A tool of the record (`podvm-gate`, `podvm-count`) finds the repository
  from its own path: in a worktree, build it in the worktree's own
  `target/`.
* Before an entry is set done, run its proof and the uid-1000 suite.
* Before a release, run `arm.yml` too: experiment 356 found T-1923, which
  the gate and the nightly trial do not cover.
* The sudo of a GitHub runner keeps `XDG_CONFIG_HOME` of the runner's
  user; an experiment that makes a second user removes each `XDG_*`.
* The disk is small: remove `target/debug` and stale `deps` when it fills,
  and the test directories under `/var/tmp`.
* A release takes the head of `main`: hold each push from the version bump
  until the release run ends, and move a change that must ship into the
  bump's section of the changelog with a new gate, `arm.yml` and nightly
  trial on its commit.
* When a test moves to another file, move the case of `podvm-plant` that
  pins it too (`crates/podvm-gate/src/plant/pins.rs` and the list in
  `plant.rs`), and run `podvm-plant` (no `--help`: any argument starts the
  full run, and a cut run can leave a plant in the tree).

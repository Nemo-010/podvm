# Handoff

The state on 2026-11-16, 16:15 (UTC), at the end of a session. `main` is at the commit that adds
this file. `0.2.0-beta.29` is the last release (release run 43913242822
on `978e38dd`). A fresh session with no memory reads this file first,
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
* podcrates is taken up to `v0.64.0` (PodUtils/podcrates#214, COORD-166,
  #459). Before it `v0.63.0` (PodUtils/podcrates#211). Since
  `v0.62.0` (PodUtils/podcrates#210): its
  `chroot_usable` is fixed (COORD-150) and `Plan::userns_ids` serves
  T-1927 (COORD-154). The way out of a network lab (T-1924), the
  installer core (T-1925) and `pod-qga` (T-1868) are taken; the served
  ports of a lab (no entry yet) are not.
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

## Done in this session (2026-11-16, the second session)

* T-1917 partial: `nightly.yml` writes `podvm.1` and `podvm-llms.txt`
  from the release binary, lints the page with `mandoc -Tlint -Wwarning`,
  and signs and attaches both; `release.yml` checks them. The nightly
  trial 43896360464 on `8ade508` passed. Remaining: a release that carries
  them, and the writer, the facts and the checks of `pod-manual`.
* T-1924 done: `podvm lab run --way-out`. The stand-in proxy of a lab died
  of SIGPIPE in podvm (a fix in `lab/host.rs`). COORD-162 (#455) asks
  podcrates for the roots of the host in the certificate file of a lab
  with a way out, and for a stand-in that protects itself from SIGPIPE.
  The Prove parser of the gate stops at a bare `--`.
* T-1925 partial: the installer is on the core of podcrates `v0.63.0`;
  the publish job signs `SHA256SUMS` (`SHA256SUMS.sigstore.json`) on the
  tag ref only. Remaining: the first release run proves it.
* T-1867 partial: a writable log stick keeps the logs of WinPE
  (`PELOGS\` on `pelogs.img`). `kvm.yml` run 43898543916 (`only=install`,
  on `c31b708`) runs it; read `install-shots/winpe/PELOGS/` in its
  artifact.
* T-1868 partial: `exec`, `cp`, a graceful `stop` and the readiness probe
  of a running Windows machine on `pod-qga`, and virtio-win 0.1.302 in the
  install chain. COORD-163 (#456) asks podcrates to list
  `fedorapeople.org`. The install now has two more devices (a CD and the
  agent channel); if the next T-1867 run differs, suspect them.
* T-1911 partial: `-u` drops for real again with the range (T-1927 had
  nested a one-line namespace); `cp` and `exec` of a stopped tree of a
  subordinate id run again in a namespace with the range. Remaining:
  `start` (see the entry), then the default.
* T-1786 done (issue 330 closed): a run that `PODVM_MODE` forces holds its
  record through `spawn_ladder`; COORD-122 (#384) closed. T-1929 (new,
  P3): the forced `rundir` and `tmpfs` rungs refuse the device nodes of
  the fixups.
* The host of a cloud session lacks `uidmap`: install it before a T-1911
  measurement (`apt-get install -y uidmap`).

## Next steps, in order

1. Start the session (`AGENTS.md`, section 8), with the watcher.
2. A new ask of a peer first. Open and waiting: #439 (podrunner), #445,
   #446, #449, #453, #455, #456, #457 (podcrates), #458 (podrunner).
3. Read `kvm.yml` run 43898543916: the WinPE logs of T-1867 give the
   error of Setup. Then fix the cause, and run the install again (it now
   carries the guest tools of T-1868).
4. `0.2.0-beta.29` is released (run 43913242822): T-1925 done, T-1917
   has its assets. T-1930 (P1, PodUtils/podrunner#200) waits for
   COORD-164 (#457); take the tag of podcrates that carries it first.
5. Then the work order of `TODO/PROGRESS.md` (T-1911 `start`, T-1929,
   T-1872, the Windows entries).

## The state at the end of the session

* `main` is pushed; read the last gate run before work.
* The open issues of podvm that wait for a peer: #439 (podrunner), #445,
  #446, #449, #453, #455, #456, #457 (podcrates), #458 (podrunner).

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

//! TODO/machine.md T-1870: `start` and `rm` of a machine, out of
//! `machine.rs` for the limit of 500 lines of a source file.

use std::time::{Duration, Instant};

use podvm_image::error::EXIT_RUNTIME_ERROR;
use podvm_windows::machines;

use super::{find, start_keeper, stop_record, STOP_GRACE};

/// `start` of `want` where a machine has that name: a running one changes
/// nothing; a stopped one boots again from its own disk. `None` for no
/// machine.
pub(crate) fn start_if(verb: &str, want: &str) -> Option<Result<String, i32>> {
    let mut r = find(want)?;
    if r.running() {
        return Some(Ok(r.name));
    }
    if r.flavor == "linux" {
        eprintln!(
            "podvm {verb}: {want} is a guest of the machine tier, whose disk is the image: \
             `podvm rm {want}` and `podvm run -d` start a new one (TODO/machine.md T-1750)"
        );
        return Some(Err(EXIT_RUNTIME_ERROR));
    }
    if machines::alive(r.keeper_pid, r.keeper_start) {
        eprintln!("podvm {verb}: the keeper of {want} still runs; wait for it to end");
        return Some(Err(EXIT_RUNTIME_ERROR));
    }
    let started = (|| {
        crate::windows::plan::provide(verb, r.flavor != "dos")?;
        let accel = crate::windows::plan::accelerator()?;
        r.ended.clear();
        r.keeper_pid = 0;
        r.keeper_start = 0;
        r.emulator_pid = 0;
        r.emulator_start = 0;
        let root = machines::root();
        machines::write(&root, &r).map_err(|e| {
            eprintln!("podvm {verb}: {want}: {e}");
            EXIT_RUNTIME_ERROR
        })?;
        start_keeper(&r.name, accel, &machines::dir_in(&root, &r.name)).map_err(|why| {
            eprintln!("podvm {verb}: {want}: {why}");
            EXIT_RUNTIME_ERROR
        })
    })();
    Some(started.map(|()| r.name.clone()))
}

/// `podvm rm [-f] NAME` of a machine.
pub(crate) fn rm(name: &str, force: bool) -> i32 {
    let root = machines::root();
    let Some(r) = machines::read(&root, name) else {
        return EXIT_RUNTIME_ERROR;
    };
    if r.running() {
        if !force {
            eprintln!(
                "podvm rm: the machine {name} runs; `podvm stop {name}` first, or `podvm rm -f {name}`"
            );
            return EXIT_RUNTIME_ERROR;
        }
        let _ = stop_record(&root, name);
    }
    // A keeper that saw its emulator end writes the end and exits.
    let t0 = Instant::now();
    while machines::alive(r.keeper_pid, r.keeper_start) && t0.elapsed() < STOP_GRACE {
        std::thread::sleep(Duration::from_millis(100));
    }
    if r.running() || machines::alive(r.keeper_pid, r.keeper_start) {
        eprintln!("podvm rm: the emulator of {name} did not end; nothing was removed");
        return EXIT_RUNTIME_ERROR;
    }
    match std::fs::remove_dir_all(machines::dir_in(&root, name)) {
        Ok(()) => {
            println!("{name}");
            0
        }
        Err(e) => {
            eprintln!("podvm rm: {name}: {e}");
            EXIT_RUNTIME_ERROR
        }
    }
}

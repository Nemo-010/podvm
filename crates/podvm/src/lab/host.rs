//! `podvm lab profiles`, `podvm lab run PROFILE[,PROFILE...] -- CMD` and
//! `podvm lab check PROFILE[,PROFILE...]`: a hostile host class as a lab on
//! this host, with no root and no guest (TODO/net.md T-1916).
//!
//! The profiles and the labs are `pod-lab` of podcrates (podcrates@T-692,
//! `v0.54.0`): its table of profiles, its check against the rows of
//! `pod-hostprobe`, and its run, which sets each part that a host with no
//! root can hold, checks the profile from inside the lab, and only then
//! runs CMD. podvm gives it its own probe children (`--probe-child`) and a
//! lab flag of its own, and keeps the list of profiles: a list runs as
//! nested labs, the first one outermost.
//!
//! ⛔ A part that does not hold stops the run with exit 125, and CMD does
//! not run.

use std::ffi::OsString;

use pod_lab::codes::{
    EXIT_CANNOT_EXEC, EXIT_FAILS, EXIT_HOLDS, EXIT_NOT_FOUND, EXIT_REFUSED, EXIT_UNSEEN,
};
use pod_lab::{Options, Profile, RunError, Settings, State, Verdict};
use podvm_image::error::EXIT_FLAG_ERROR;

/// The private argument with which the lab process starts.
pub const LAB_FLAG: &str = "--podvm-lab-process";

/// The settings of a lab of podvm: podvm's probe children, its lab flag,
/// and the table of `pod-lab`.
pub fn settings() -> Settings {
    Settings {
        hostprobe: podvm_probe::settings(),
        lab_flag: LAB_FLAG.to_string(),
        profiles: pod_lab::PROFILES,
    }
}

/// The lab process: first in `main`, with one thread. Returns where the
/// arguments do not ask for it.
pub fn dispatch() {
    if std::env::args_os().nth(1).is_some_and(|a| a == LAB_FLAG) {
        pod_lab::dispatch(&settings());
    }
}

/// Is each name of the list `A,B` a profile?
pub fn is_profile(list: &str) -> bool {
    parse(list).is_ok()
}

/// The profiles of a list `A,B`, in its order.
pub fn parse(list: &str) -> Result<Vec<&'static Profile>, String> {
    list.split(',')
        .map(|n| {
            pod_lab::find(pod_lab::PROFILES, n)
                .ok_or_else(|| format!("{n} is not a profile; `podvm lab profiles` lists them"))
        })
        .collect()
}

pub const RUN_USAGE: &str = "usage: podvm lab run [--allow-connect PORT[,PORT...]] PROFILE[,PROFILE...] -- CMD [ARG...]\n\n\
Run CMD in a hostile host class on this host, with no root and no guest.\n\
`podvm lab profiles` lists the profiles. The lab sets each part, checks the\n\
profile from inside, and only then runs CMD; a part that does not hold\n\
stops the run with exit 125. A list runs as nested labs, the profiles of a\n\
tmpfs outermost. The exit code is\n\
the code of CMD (127: CMD was not found, 126: it did not run).\n";

pub const CHECK_USAGE: &str = "usage: podvm lab check PROFILE[,PROFILE...]\n\n\
Measure this host against each profile: one line for each part (holds,\n\
fails, or unseen) with the rows of the probe. Exit 0 when each part holds,\n\
1 when one fails, 3 when one cannot be seen here.\n";

/// The list of `podvm lab profiles`: each profile and its parts.
pub fn profiles_text() -> String {
    let mut s = String::new();
    for p in pod_lab::PROFILES {
        s.push_str(&format!("{:<20} {} ({})\n", p.name, p.about, p.source));
        for part in p.parts {
            s.push_str(&format!(
                "  {:<22} {} [{}]\n",
                part.name,
                part.about,
                part.hold.words()
            ));
        }
    }
    s
}

fn print(v: &Verdict) {
    for p in &v.parts {
        println!("{:<7} {}/{}", p.state.word(), v.profile, p.part);
        for s in &p.seen {
            let errno = s.errno.map(|e| format!(" errno={e}")).unwrap_or_default();
            println!(
                "        {:<7} {}{errno}: {}",
                s.state.word(),
                s.what,
                s.detail
            );
        }
    }
}

fn code(v: &Verdict) -> i32 {
    if v.fails() {
        EXIT_FAILS
    } else if v.parts.iter().any(|p| p.state == State::Unseen) {
        EXIT_UNSEEN
    } else {
        EXIT_HOLDS
    }
}

/// `podvm lab check`.
pub fn check(args: &[String]) -> i32 {
    let [list] = args else {
        eprintln!("{CHECK_USAGE}");
        return EXIT_FLAG_ERROR;
    };
    let profiles = match parse(list) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("podvm lab check: {e}");
            return EXIT_FLAG_ERROR;
        }
    };
    let s = settings();
    let mut worst = EXIT_HOLDS;
    for p in profiles {
        let v = pod_lab::check(p, &pod_lab::measure(&s.hostprobe, &pod_lab::rows(p)));
        print(&v);
        worst = match (worst, code(&v)) {
            (EXIT_FAILS, _) | (_, EXIT_FAILS) => EXIT_FAILS,
            (EXIT_UNSEEN, _) | (_, EXIT_UNSEEN) => EXIT_UNSEEN,
            _ => EXIT_HOLDS,
        };
    }
    worst
}

/// The order of the nested labs: a profile that holds a tmpfs opens a user
/// namespace, which an outer `userns-off` refuses (EPERM), so each such
/// profile is outermost; the others keep the order of the list.
pub fn nest_order(mut p: Vec<&'static Profile>) -> Vec<&'static Profile> {
    p.sort_by_key(|p| {
        !p.parts
            .iter()
            .any(|x| matches!(x.hold, pod_lab::Hold::Tmpfs { .. }))
    });
    p
}

/// The command of the outermost lab: CMD itself, or for a list, podvm
/// again with the rest of the list. `/proc/self/exe` is podvm in the lab
/// process, also where a tmpfs of `small-tmp` hides a podvm under `/tmp`.
pub fn inner(rest: &[&Profile], allow: &[u16], cmd: &[String]) -> Vec<OsString> {
    let mut v: Vec<OsString> = Vec::new();
    if !rest.is_empty() {
        let names: Vec<&str> = rest.iter().map(|p| p.name).collect();
        v.extend(["/proc/self/exe", "lab", "run"].map(OsString::from));
        if !allow.is_empty() {
            let ports: Vec<String> = allow.iter().map(u16::to_string).collect();
            v.push("--allow-connect".into());
            v.push(ports.join(",").into());
        }
        v.push(names.join(",").into());
        v.push("--".into());
    }
    v.extend(cmd.iter().map(OsString::from));
    v
}

/// `podvm lab run`.
pub fn run(args: &[String]) -> i32 {
    // `--allow-connect PORT[,PORT...]`: the ports that `bind-denied` lets a
    // TCP connect reach (pod-lab `v0.58.0`, podcrates@T-737): the proxy of
    // a host whose only way out is that proxy.
    let (allow, args) = match args {
        [f, ports, rest @ ..] if f == "--allow-connect" => (ports.as_str(), rest),
        [f, rest @ ..] if f.starts_with("--allow-connect=") => {
            (&f["--allow-connect=".len()..], rest)
        }
        _ => ("", args),
    };
    let allow_connect: Vec<u16> = match allow
        .split(',')
        .filter(|p| !p.is_empty())
        .map(str::parse)
        .collect()
    {
        Ok(v) => v,
        Err(_) => {
            eprintln!(
                "podvm lab run: --allow-connect {allow}: each port is a number from 1 to 65535"
            );
            return EXIT_FLAG_ERROR;
        }
    };
    let (list, cmd) = match args {
        [list, dash, cmd @ ..] if dash == "--" && !cmd.is_empty() => (list, cmd),
        _ => {
            eprintln!("{RUN_USAGE}");
            return EXIT_FLAG_ERROR;
        }
    };
    let profiles = match parse(list) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("podvm lab run: {e}");
            return EXIT_FLAG_ERROR;
        }
    };
    let profiles = nest_order(profiles);
    let (first, rest) = (profiles[0], &profiles[1..]);
    let opts = Options {
        partial: false,
        stdout: None,
        allow_connect: allow_connect.clone(),
    };
    match pod_lab::run(&settings(), first, &inner(rest, &allow_connect, cmd), opts) {
        Ok(ran) => pod_lab::exit_code(ran.status),
        Err(RunError::Refused(v)) => {
            print(&v);
            eprintln!(
                "podvm lab run: the lab {} does not hold here; the command did not run",
                first.name
            );
            EXIT_REFUSED
        }
        Err(RunError::Exec { errno, .. }) => {
            eprintln!(
                "podvm lab run: {}: the command did not start: errno {errno}",
                cmd[0]
            );
            if errno == 2 {
                EXIT_NOT_FOUND
            } else {
                EXIT_CANNOT_EXEC
            }
        }
        Err(e) => {
            eprintln!("podvm lab run: {e}");
            EXIT_REFUSED
        }
    }
}

#[cfg(test)]
mod tests;

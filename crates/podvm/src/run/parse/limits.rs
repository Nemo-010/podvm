//! PodUtils/podrunner#110, TODO/machine.md T-1897: `--memory`, `--cpus`
//! and `--pids-limit`. The incus tier holds them as the keys of the
//! instance (`limits.memory`, `limits.cpu`, `limits.processes`); each other
//! tier refuses them by name, as a limit with no cgroup is not held
//! (the cgroup route is TODO/cli.md T-1747).

use podvm_image::error::EXIT_FLAG_ERROR;

/// One flag and its value as the incus key and value.
pub(crate) fn incus_limit(verb: &str, flag: &str, v: &str) -> Result<(&'static str, String), i32> {
    let bad = |why: &str| {
        eprintln!("podvm {verb}: {flag} {v:?}: {why} (TODO/machine.md T-1897)");
        EXIT_FLAG_ERROR
    };
    match flag {
        "--memory" => {
            let (num, mult) = match v.to_ascii_lowercase().chars().last() {
                Some('k') => (&v[..v.len() - 1], 1u64 << 10),
                Some('m') => (&v[..v.len() - 1], 1 << 20),
                Some('g') => (&v[..v.len() - 1], 1 << 30),
                Some('b') => (&v[..v.len() - 1], 1),
                _ => (v, 1),
            };
            let n: u64 = num
                .parse()
                .map_err(|_| bad("takes bytes with b, k, m or g"))?;
            let bytes = n.checked_mul(mult).filter(|b| *b >= 6 << 20);
            let bytes = bytes.ok_or_else(|| bad("takes at least 6m, as docker does"))?;
            Ok(("limits.memory", format!("{bytes}B")))
        }
        "--cpus" => {
            let c: f64 = v.parse().map_err(|_| bad("takes a number of CPUs"))?;
            if !(c > 0.0 && c <= 1024.0) {
                return Err(bad("takes more than 0 and at most 1024 CPUs"));
            }
            // A whole number is a count of CPUs; a part is a share of time.
            if c.fract() == 0.0 {
                Ok(("limits.cpu", format!("{}", c as u64)))
            } else {
                Ok((
                    "limits.cpu.allowance",
                    format!("{}%", (c * 100.0).round() as u64),
                ))
            }
        }
        _ => {
            let n: u64 = v.parse().map_err(|_| bad("takes a count of processes"))?;
            if n == 0 {
                return Err(bad("takes 1 or more"));
            }
            Ok(("limits.processes", n.to_string()))
        }
    }
}

/// Refuse the limits on a tier that does not hold them.
pub(crate) fn refuse_off_incus(
    verb: &str,
    limits: &[(String, &'static str, String)],
) -> Result<(), i32> {
    if let Some((flag, _, _)) = limits.first() {
        eprintln!(
            "podvm {verb}: {flag} needs a cgroup, which this tier does not hold: the incus \
             tier (--podvm-tier=incus) holds it as a limit of the instance; --ulimit nproc= \
             and --ulimit as= hold on each tier (TODO/machine.md T-1897, TODO/cli.md T-1747)"
        );
        return Err(EXIT_FLAG_ERROR);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incus_limits_map_the_docker_values() {
        assert_eq!(
            incus_limit("run", "--memory", "512m").unwrap(),
            ("limits.memory", "536870912B".to_string())
        );
        assert_eq!(
            incus_limit("run", "--cpus", "2").unwrap(),
            ("limits.cpu", "2".to_string())
        );
        assert_eq!(
            incus_limit("run", "--cpus", "1.5").unwrap(),
            ("limits.cpu.allowance", "150%".to_string())
        );
        assert_eq!(
            incus_limit("run", "--pids-limit", "64").unwrap(),
            ("limits.processes", "64".to_string())
        );
        assert!(incus_limit("run", "--memory", "1k").is_err());
        assert!(incus_limit("run", "--cpus", "0").is_err());
        assert!(incus_limit("run", "--pids-limit", "x").is_err());
        assert!(refuse_off_incus("run", &[]).is_ok());
        let l = vec![("--memory".to_string(), "limits.memory", "1B".to_string())];
        assert!(refuse_off_incus("run", &l).is_err());
    }
}

//! Local capacity measurement with a bounded observational subprocess.
use super::{Budget, Checks};
use layerfs_bridge::control::SdkValue;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub(super) fn check(home: &Path, budget: &Budget, checks: &mut Checks) {
    let mut path = home;
    while !path.exists() {
        let Some(parent) = path.parent() else {
            checks.unknown(
                "host.capacity",
                "No existing filesystem ancestor for the home",
            );
            return;
        };
        path = parent;
    }
    let wait = budget.limit(Duration::from_secs(5));
    if wait.is_zero() {
        checks.unknown(
            "host.capacity",
            "Deadline expired before measuring capacity",
        );
        return;
    }
    let started = Instant::now();
    let child = Command::new("/bin/df")
        .args(["-Pk"])
        .arg(path)
        .env("LC_ALL", "C")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else {
        checks.unknown("host.capacity", "Filesystem capacity probe is unavailable");
        return;
    };
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < wait => std::thread::sleep(Duration::from_millis(5)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                checks.unknown(
                    "host.capacity",
                    "Filesystem capacity probe did not complete within its deadline",
                );
                return;
            }
        }
    }
    let capacity = child
        .wait_with_output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| {
            let text = std::str::from_utf8(&out.stdout).ok()?;
            let fields: Vec<_> = text.lines().nth(1)?.split_whitespace().collect();
            let bytes = fields.get(3)?.parse::<u64>().ok()?.checked_mul(1024)?;
            Some((fields.first()?.to_string(), bytes))
        });
    match capacity {
        Some((filesystem, bytes)) => {
            checks.pass(
                "host.capacity",
                "Measured local filesystem capacity; no product free-space threshold applied",
            );
            checks.evidence(
                "host.capacity",
                SdkValue::map([
                    ("filesystem", SdkValue::text(filesystem)),
                    ("path", SdkValue::text(path.display().to_string())),
                    (
                        "free_bytes",
                        i64::try_from(bytes).map_or(SdkValue::Null, SdkValue::Int),
                    ),
                ]),
            );
        }
        None => checks.unknown(
            "host.capacity",
            "Filesystem capacity probe returned no usable measurement",
        ),
    }
    checks.duration("host.capacity", started.elapsed());
}

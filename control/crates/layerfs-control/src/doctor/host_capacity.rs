//! `home.capacity`: free space on the disk that holds the home, measured
//! by one bounded `df` and judged by the doctors' one room rule (a nearly
//! full disk is a warning).
use super::{Budget, Checks};
use layerfs_bridge::control::SdkValue;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const ID: &str = "home.capacity";

pub(super) fn check(home: &Path, budget: &Budget, checks: &mut Checks) {
    let mut path = home;
    while !path.exists() {
        let Some(parent) = path.parent() else {
            checks.unknown(ID, "no folder above the home exists to measure");
            return;
        };
        path = parent;
    }
    let wait = budget.limit(Duration::from_secs(5));
    if wait.is_zero() {
        checks.timeout(ID, "its time ran out before the disk was measured");
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
        checks.unknown(ID, "`df` could not be run to measure the disk");
        return;
    };
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < wait => std::thread::sleep(Duration::from_millis(5)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                checks.timeout(ID, "`df` did not answer in time");
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
        Some((filesystem, free)) => super::super::capacity::room(
            checks,
            ID,
            "the disk that holds the home",
            free,
            None,
            vec![
                ("filesystem", SdkValue::text(filesystem)),
                ("path", SdkValue::text(path.display().to_string())),
            ],
        ),
        None => checks.unknown(ID, "`df` gave no measurement of the disk"),
    }
}

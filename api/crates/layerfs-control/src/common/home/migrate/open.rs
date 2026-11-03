//! Opening the home: created when empty, converted once from 1.1.0
//! (`v110.rs`) under the home lock, by the first call on that home.
use crate::common::home::layout::{create, detect, HomeFile, Layout};
use crate::common::home::paths;
use crate::common::home::records::{log_line, now_ms, write_record};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The home ready for 2.0 calls: created when empty, converted once when it
/// is a 1.1.0 home. Answers its `layerfs.json`. A worker's root is not a
/// home: it answers the id its caller sent, and nothing is written.
pub fn open(home: &Path) -> Result<HomeFile> {
    open_from(home, Path::new(super::volume::DOCKER_VOLUMES))
}

/// [`open`], with a 1.1.0 home's Docker volumes read from `volumes` (1.1.0's
/// `volumes` setting; a rootless Docker keeps them elsewhere).
pub fn open_from(home: &Path, volumes: &Path) -> Result<HomeFile> {
    if crate::common::home::guest::is_root(home) {
        return crate::common::home::guest::file(home);
    }
    let file = ready(home, volumes)?;
    crate::common::home::guest::opened(&file.id);
    Ok(file)
}

fn ready(home: &Path, volumes: &Path) -> Result<HomeFile> {
    match detect(home)? {
        Layout::Current(file) => Ok(file),
        Layout::Empty | Layout::Old => {
            let _lock = crate::common::home::lock::take(home)?;
            match detect(home)? {
                Layout::Current(file) => Ok(file),
                Layout::Empty => create(home),
                Layout::Old => {
                    let report = super::v110::convert(home, volumes)?;
                    let file = create(home)?;
                    write_record(&paths::log_dir(home).join("migration.json"), &report)?;
                    Ok(file)
                }
            }
        }
    }
}

/// Logs a line for a conversion that could not finish (the next call
/// continues it).
pub fn note_failure(home: &Path, refusal: &SdkRefusal) {
    let _ = log_line(
        home,
        "migration-failures.jsonl",
        &SdkValue::map([
            ("at", SdkValue::Int(now_ms())),
            ("code", SdkValue::text(&refusal.code)),
            ("message", SdkValue::text(&refusal.message)),
        ]),
    );
}

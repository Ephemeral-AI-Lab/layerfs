//! Opening the home: created when empty, converted once from 1.1.0
//! (`v110.rs`) and from layout 2 (`v2.rs`) under the home lock, by the
//! first call on that home.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::layout::{create, detect, write, HomeFile, Layout, PREVIOUS_LAYOUT};
use layerfs_home::home::paths;
use layerfs_home::home::records::{log_line, now_ms, write_record};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The home ready for calls: created when empty, converted once when it is
/// a 1.1.0 or layout 2 home (refused `InUse` while anything in a layout 2
/// home is live, `v2.rs`). Answers its `layerfs.json`. A worker's root is not a
/// home: it answers the id its caller sent, and nothing is written.
pub fn open(home: &Path) -> Result<HomeFile> {
    open_from(home, Path::new(super::volume::DOCKER_VOLUMES))
}

/// [`open`], with a 1.1.0 home's Docker volumes read from `volumes` (1.1.0's
/// `volumes` setting; a rootless Docker keeps them elsewhere).
pub fn open_from(home: &Path, volumes: &Path) -> Result<HomeFile> {
    if layerfs_home::home::guest::is_root(home) {
        return layerfs_home::home::guest::file(home);
    }
    let file = ready(home, volumes)?;
    layerfs_home::home::guest::opened(&file.id);
    Ok(file)
}

fn ready(home: &Path, volumes: &Path) -> Result<HomeFile> {
    match detect(home)? {
        Layout::Current(file) => {
            super::v2::finish(home)?;
            Ok(file)
        }
        Layout::Empty | Layout::Old | Layout::Previous(_) => {
            let _lock = layerfs_home::home::lock::take(home)?;
            match detect(home)? {
                Layout::Current(file) => Ok(file),
                Layout::Empty => create(home),
                Layout::Previous(file) => super::v2::convert(home, &file),
                Layout::Old => {
                    // 1.1.0 converts to layout 2, which converts on
                    let report = super::v110::convert(home, volumes)?;
                    let mut file = create(home)?;
                    file.layout = PREVIOUS_LAYOUT;
                    write(home, &file)?;
                    write_record(&paths::log_dir(home).join("migration.json"), &report)?;
                    super::v2::convert(home, &file)
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

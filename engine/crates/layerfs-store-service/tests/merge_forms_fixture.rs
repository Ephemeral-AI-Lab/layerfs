//! The root-form Store behind `merge_mixed_forms` (#355 M1 x #360). Run on
//! a build before #360 item 2, where every inode value carries its metadata
//! in the root form: it seals a base tree and a Branch `r` changed in that
//! form (a move, two content edits and a mode change), for a build with the
//! inline form to merge against. The output is
//! checked in at `tests/fixtures/merge-forms-root.sqlite` on the branch that
//! carries #360.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use std::fs;
use support::{write, Fixture};

/// Entries below the root of the base tree.
pub const ENTRIES: usize = 14;

fn mv(from: &str, to: &str) -> SdkValue {
    SdkValue::map([
        ("op", SdkValue::text("mv")),
        ("from", SdkValue::text(from)),
        ("to", SdkValue::text(to)),
    ])
}

#[test]
#[ignore = "setup: writes the root-form Store to LAYERFS_355_FORMS_FIXTURE"]
fn prepare_root_form_fixture() {
    let out = std::env::var_os("LAYERFS_355_FORMS_FIXTURE").expect("output path");
    let fixture = Fixture::new("merge-forms-prepare", |source| {
        for directory in ["d1", "f", "h"] {
            fs::create_dir(source.join(directory)).unwrap();
        }
        fs::write(source.join("d1/a.txt"), b"a\n").unwrap();
        fs::write(source.join("d1/b.txt"), b"b\n").unwrap();
        for name in ["same", "rmode", "mode", "edit", "other", "both"] {
            fs::write(source.join(format!("f/{name}.txt")), b"1\n2\n3\n").unwrap();
        }
        fs::write(source.join("h/one"), b"linked\n").unwrap();
        fs::hard_link(source.join("h/one"), source.join("h/two")).unwrap();
        fs::write(source.join("top.txt"), b"top\n").unwrap();
        ENTRIES
    });
    fixture.fork("r");
    fixture.apply(
        "r",
        vec![
            mv("/d1", "/moved"),
            write("/f/other.txt", "1\n2\nother, r\n"),
            write("/f/both.txt", "1\n2\nboth, r\n"),
        ],
    );
    // A real mode change in the root form, against which the other side
    // makes a form-only change.
    fixture.chmod("r", "/f/rmode.txt", 0o600);
    let host = fixture.close();
    fs::copy(&host.config.path, &out).unwrap();
    println!(
        "MERGE_FORMS_FIXTURE {:?} bytes={}",
        out,
        fs::metadata(&out).unwrap().len()
    );
    host.cleanup();
}

//! External compile-fail checks for scope lifetime and thread restrictions.
//!
//! Every fixture under `tests/compile_fail/` is compiled by a real `cargo check`
//! in a throwaway crate outside this workspace, so the checks exercise the same
//! public API and toolchain a consumer uses. The control fixture must build;
//! each restricted usage must fail with its intended error code.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A fixture that must not compile, with the diagnostic it must produce.
struct Restricted {
    file: &'static str,
    code: &'static str,
    fragment: &'static str,
}

const RESTRICTED: [Restricted; 6] = [
    Restricted {
        file: "escape_child_scope.rs",
        code: "E0521",
        fragment: "escapes",
    },
    Restricted {
        file: "send_scope_across_threads.rs",
        code: "E0277",
        fragment: "cannot be sent between threads safely",
    },
    Restricted {
        file: "child_on_pending_scope.rs",
        code: "E0599",
        fragment: "no method named `child`",
    },
    Restricted {
        file: "attach_on_pending_scope.rs",
        code: "E0599",
        fragment: "no method named `attach`",
    },
    Restricted {
        file: "run_scope_twice.rs",
        code: "E0382",
        fragment: "use of moved value",
    },
    Restricted {
        file: "run_active_handle.rs",
        code: "E0599",
        fragment: "no method named `run`",
    },
];

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture_dir() -> PathBuf {
    crate_dir().join("tests").join("compile_fail")
}

fn scratch_dir() -> PathBuf {
    std::env::temp_dir().join(format!(
        "layerfs-telemetry-compile-fail-src-{}",
        std::process::id()
    ))
}

fn target_dir() -> PathBuf {
    std::env::temp_dir().join(format!(
        "layerfs-telemetry-compile-fail-target-{}",
        std::process::id()
    ))
}

fn check_fixture(file: &str, target: &Path) -> Output {
    let name = file.trim_end_matches(".rs");
    let scratch = scratch_dir().join(name);
    std::fs::create_dir_all(scratch.join("src")).expect("fixture source directory");
    // A distinct package name per fixture is required: the target directory is
    // shared to reuse the compiled dependency, and cargo treats identical
    // name/version packages as the same unit even from different directories.
    let package = format!("fixture-{}", name.replace('_', "-"));
    let manifest = format!(
        "[package]\nname = \"{package}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[workspace]\n\n[dependencies]\nlayerfs-telemetry = {{ path = \"{}\" }}\n",
        crate_dir().display()
    );
    std::fs::write(scratch.join("Cargo.toml"), manifest).expect("fixture manifest");
    std::fs::copy(
        fixture_dir().join(file),
        scratch.join("src").join("main.rs"),
    )
    .expect("fixture source");
    Command::new(env!("CARGO"))
        .arg("check")
        .arg("--offline")
        .arg("--quiet")
        .current_dir(&scratch)
        .env("CARGO_TARGET_DIR", target)
        .output()
        .expect("run cargo check for a compile-fail fixture")
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn external_compile_fail_checks() {
    let target = target_dir();
    let mut failures = Vec::new();

    let control = check_fixture("control_injected_scopes.rs", &target);
    if !control.status.success() {
        failures.push(format!(
            "control_injected_scopes.rs must compile but failed:\n{}",
            stderr_of(&control)
        ));
    }

    for case in RESTRICTED {
        let output = check_fixture(case.file, &target);
        let stderr = stderr_of(&output);
        if output.status.success() {
            failures.push(format!("{} unexpectedly compiled", case.file));
            continue;
        }
        if !stderr.contains(case.code) {
            failures.push(format!(
                "{} did not report {}:\n{stderr}",
                case.file, case.code
            ));
        }
        if !stderr.contains(case.fragment) {
            failures.push(format!(
                "{} did not report `{}`:\n{stderr}",
                case.file, case.fragment
            ));
        }
    }

    let _ = std::fs::remove_dir_all(scratch_dir());
    let _ = std::fs::remove_dir_all(&target);
    assert!(
        failures.is_empty(),
        "external compile-fail checks failed:\n{}",
        failures.join("\n")
    );
}

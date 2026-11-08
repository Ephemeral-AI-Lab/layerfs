//! Embed the source identity used to compile the CLI.
use std::process::Command;

fn git(args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .output()
        .expect("run git for CLI identity");
    assert!(
        output.status.success(),
        "git failed while identifying CLI source"
    );
    String::from_utf8(output.stdout)
        .expect("UTF-8 git identity")
        .trim()
        .to_owned()
}

fn main() {
    for name in ["LAYERFS_BUILD_COMMIT", "LAYERFS_BUILD_DIRTY"] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    if let Ok(commit) = std::env::var("LAYERFS_BUILD_COMMIT") {
        let dirty =
            std::env::var("LAYERFS_BUILD_DIRTY").expect("build dirty state accompanies commit");
        assert!(
            commit.len() == 40 && commit.bytes().all(|b| b.is_ascii_hexdigit()),
            "full build commit"
        );
        assert!(
            matches!(dirty.as_str(), "true" | "false"),
            "boolean build dirty state"
        );
        println!("cargo:rustc-env=LAYERFS_BUILD_COMMIT={commit}");
        println!("cargo:rustc-env=LAYERFS_BUILD_DIRTY={dirty}");
        return;
    }
    // Git's index and refs can change without any Rust input changing.
    // Recheck on every build, including linked worktrees and packed refs.
    let root = git(&["rev-parse", "--show-toplevel"]);
    for path in git(&["ls-files"]).lines() {
        println!("cargo:rerun-if-changed={root}/{path}");
    }
    for name in ["HEAD", "index", "packed-refs"] {
        println!(
            "cargo:rerun-if-changed={}",
            git(&["rev-parse", "--git-path", name])
        );
    }
    let reference = git(&["rev-parse", "--symbolic-full-name", "HEAD"]);
    println!(
        "cargo:rerun-if-changed={}",
        git(&["rev-parse", "--git-path", &reference])
    );
    println!(
        "cargo:rustc-env=LAYERFS_BUILD_COMMIT={}",
        git(&["rev-parse", "HEAD"])
    );
    println!(
        "cargo:rustc-env=LAYERFS_BUILD_DIRTY={}",
        !git(&["status", "--porcelain", "--untracked-files=normal"]).is_empty()
    );
}

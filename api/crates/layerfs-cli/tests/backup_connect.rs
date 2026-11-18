//! `layerfs backup connect <name> <url> --access-key K --key-stdin` on the
//! command line (call surface 2.0): the key's secret is read from stdin
//! through `--key-stdin` and taken no other way; today's `--key` reference,
//! `--region` and `--part-size` are gone with no alias; a bucket that does
//! not answer is `Unavailable` and nothing is recorded.
mod common;
use common::{layerfs, run, run_with_stdin, tempdir, text};
use layerfs_cli::parse::parse;

fn words(line: &[&str]) -> Vec<String> {
    line.iter().map(|word| (*word).to_owned()).collect()
}

#[test]
fn the_secret_comes_on_stdin_and_old_options_are_gone() {
    let connect = layerfs_api::find("backup.connect").expect("backup.connect");
    let line = words(&[
        "b1",
        "http://127.0.0.1:9/lfs-backups/p",
        "--access-key",
        "AKID",
        "--key-stdin",
    ]);
    let parsed = parse(connect, &line, &mut &b"s3cr3t\n"[..]).unwrap_or_else(|u| panic!("{}", u.0));
    assert_eq!(parsed.args.get_text("access_key"), Some("AKID"));
    assert_eq!(parsed.args.get_text("secret_key"), Some("s3cr3t"));
    for line in [
        &["b1", "http://h/b", "--secret-key", "s3cr3t"][..],
        &["b1", "http://h/b", "--key", "env:LAYERFS_X"],
        &["b1", "http://h/b", "--region", "auto"],
        &["b1", "http://h/b", "--part-size", "8M"],
    ] {
        let refused = parse(connect, &words(line), &mut &b""[..]).err().unwrap().0;
        assert!(!refused.contains("s3cr3t"), "{line:?}: {refused}");
    }
    // both key options are required: a line without one is wrong (exit 2)
    for (line, missing) in [
        (
            &["b1", "http://h/b", "--key-stdin"][..],
            "--access-key is required",
        ),
        (
            &["b1", "http://h/b", "--access-key", "AKID"],
            "--key-stdin is required",
        ),
    ] {
        let refused = parse(connect, &words(line), &mut &b"s3cr3t"[..])
            .err()
            .unwrap()
            .0;
        assert!(refused.contains(missing), "{line:?}: {refused}");
    }
    // help shows them without brackets
    let usage = layerfs_cli::help::usage(connect);
    assert!(
        usage.ends_with("<name> <url> --access-key <access-key> --key-stdin"),
        "{usage}"
    );
    // restore names the bucket, its file, then the Store
    let restore = layerfs_api::find("backup.restore").expect("backup.restore");
    let parsed = parse(restore, &words(&["b1", "shop", "copy"]), &mut &b""[..])
        .unwrap_or_else(|u| panic!("{}", u.0));
    assert_eq!(parsed.args.get_text("bucket"), Some("b1"));
    assert_eq!(parsed.args.get_text("file"), Some("shop"));
    assert_eq!(parsed.args.get_text("store"), Some("copy"));
}

#[test]
fn a_bucket_that_does_not_answer_is_unavailable_and_nothing_is_recorded() {
    let root = tempdir("backup-connect");
    let home = root.join("home");
    let out = run(&mut layerfs(&home, &["home", "init"]));
    assert!(out.status.success(), "{out:?}");
    let out = run_with_stdin(
        &mut layerfs(
            &home,
            &[
                "backup",
                "connect",
                "b1",
                "http://127.0.0.1:9/lfs-backups/p",
                "--access-key",
                "AKID",
                "--key-stdin",
            ],
        ),
        b"s3cr3t\n",
    );
    assert_eq!(out.status.code(), Some(125), "{out:?}");
    let refusal = text(&out.stderr);
    assert!(refusal.contains("Unavailable"), "{refusal}");
    assert!(!refusal.contains("s3cr3t"), "{refusal}");
    assert!(!home.join("backup/b1").exists());
    assert!(!home.join("secret/bucket/b1.json").exists());
    let _ = std::fs::remove_dir_all(root);
}

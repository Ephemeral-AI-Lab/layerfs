//! Real kernel mounts: an explicit control Commit over changes that ordinary,
//! externally launched processes made through plain filesystem syscalls. Each
//! published root is read back completely from a fresh bind and through a
//! fresh mount, and compared with the tree as the mount showed it before the
//! Commit. Counted work is printed; nothing is timed.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/complete_bytes.rs"]
mod bytes;
#[allow(dead_code)]
#[path = "support/complete_fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "support/installed_store.rs"]
mod installed;
#[allow(dead_code)]
#[path = "support/namespace_model.rs"]
mod model;
#[allow(dead_code)]
#[path = "support/mounted.rs"]
mod mounted;
#[allow(dead_code)]
#[path = "support/mutating.rs"]
mod mutating;
#[allow(dead_code)]
#[path = "support/mounted_commit.rs"]
mod rig;
#[allow(dead_code)]
#[path = "support/native_install.rs"]
mod support;
use layerfs_bridge::control::Activity;
use model::{assert_same, Flat, Seen, DIRECTORY, FILE, SYMLINK};
use mounted::{until, COMMAND};
use mutating::{counter, direct, Mapping};
use nix::libc;
use rig::{
    assert_kernel, bash, bash_in, clock_masked, clocked, kernel, native, passed, pattern, root,
    Background, Rig,
};
use std::{
    fs::{self, OpenOptions},
    os::unix::process::CommandExt,
    process::{Command, Stdio},
};

const MAP_STORE: &str = "LAYERFS_R5_MAP_CLOSE_STORE_EXIT";
const PAGE: usize = 4096;

fn at<'a>(flat: &'a Flat, path: &str) -> &'a Seen {
    flat.get(path.as_bytes())
        .unwrap_or_else(|| panic!("missing {path}"))
}
fn file<'a>(flat: &'a Flat, path: &str) -> &'a [u8] {
    let seen = at(flat, path);
    assert_eq!(seen.kind, FILE, "{path}");
    &seen.payload
}
/// Whole-content equality that reports where two byte strings part, not
/// the strings.
fn same_bytes(actual: &[u8], expected: &[u8], what: &str) {
    let first = actual
        .iter()
        .zip(expected)
        .position(|(a, b)| a != b)
        .unwrap_or(actual.len().min(expected.len()));
    assert!(
        actual == expected,
        "{what}: {} bytes against {} expected, first difference at {first}",
        actual.len(),
        expected.len()
    );
}
fn absent(flat: &Flat, paths: &[&str]) {
    for path in paths {
        assert!(!flat.contains_key(path.as_bytes()), "{path} is still there");
    }
}

/// Every kind of ordinary change, in the tree, inside `.git`, an ignored
/// directory, the cache and the build output. Run with the tree as the
/// working directory, once on the mount and once on a native copy.
const EVERY_KIND: &str = r#"
set -euo pipefail
umask 022
# Create: files, directories and symlinks.
mkdir -p made/sub/leaf .git/objects/cc ignored/new-dir .cache/tool/tmp target/release dist/assets/img
printf 'created at top\n' > made/top.txt
printf 'new git object\n' > .git/objects/cc/ddeeff
printf 'ignored fresh\n' > ignored/new-dir/fresh.env
printf 'cache entry\n' > .cache/tool/tmp/entry
head -c 150000 /dev/zero | tr '\0' 'R' > target/release/app
printf 'asset' > dist/assets/img/a.png
: > made/sub/empty
ln -s ../top.txt made/sub/rel-link
ln -s nowhere/at/all made/dangling
ln -s sub made/dir-link
cp target/debug/build/out.o made/out-copy.o
# Overwrite: inside a large base file without truncating, and a whole file.
printf 'OVERWRITTEN' | dd of=target/debug/app bs=1 seek=123456 conv=notrunc status=none
printf 'fn main() { changed(); }\n' > src/main.rs
# Append: to a hard-linked file, an ignored file and the index.
printf 'appended line\n' >> README.md
printf 'ignored append\n' >> debug.log
printf 'more index' >> .git/index
# Truncate shorter, and to nothing.
truncate -s 1000 .cache/tool/data.bin
truncate -s 0 dist/bundle.js
# Extend with a hole: a base file, and a file made above.
truncate -s 70000 docs/guide.md
printf 'tail' | dd of=made/top.txt bs=1 seek=9000 conv=notrunc status=none
# Mode.
chmod 0600 src/lib.rs
chmod 0755 src/main.rs
chmod 0700 docs
chmod 0555 empty-dir
# Modification time: a file, two directories and a file made above.
touch -d '2001-09-09 01:46:40.123456789 UTC' node_modules/pkg/package.json
touch -d '2004-01-10 13:37:04 UTC' src/deep/a
touch -d '2010-10-10 10:10:10.000000001 UTC' made/sub/empty
touch -d '1999-12-31 23:59:59 UTC' target/debug/build
# Rename: a file in place, a file across directories, a directory in place,
# an emptied directory across directories, a populated one across directories.
mv -T src/deep/a/b/c/leaf.txt src/deep/a/b/c/leaf-renamed.txt
mv -T node_modules/pkg/lib/util.js .cache/tool/util.js
mv -T spare/inner spare/inner-renamed
mv -T node_modules/pkg/lib target/lib-moved
mv -T src/deep docs/deep-moved
# Hard link.
ln target/release/app dist/app-link
ln .git/index .git/index.bak
ln src/main.rs made/main-alias.rs
ln docs/zero made/zero-alias
# Unlink: files, an ignored file, a symlink, one of three aliases.
rm -f spare/gone.txt ignored/secret.env readme-link .cache/tool/object-alias __pycache__/app.cpython-312.pyc
# Remove directories.
rmdir __pycache__ made/sub/leaf
# Replace a file by a new one; one of them is an alias of a hard-linked file.
printf 'ref: refs/heads/topic\n' > .git/HEAD.lock
mv -T .git/HEAD.lock .git/HEAD
printf '{"replaced":true}\n' > node_modules/pkg/index.js.tmp
mv -T node_modules/pkg/index.js.tmp node_modules/pkg/index.js
printf 'pub fn replaced() {}\n' > src/lib.rs.new
mv -T src/lib.rs.new src/lib.rs
# Remove a name and create it again: as a file, and as a directory.
rm -f .gitignore
printf 'target/\n' > .gitignore
rm -f dist/assets/logo.svg
mkdir dist/assets/logo.svg
printf 'x' > dist/assets/logo.svg/inner
# Made and removed inside one capture: nothing remains.
mkdir tmp-scratch
printf 't' > tmp-scratch/t
rm -f tmp-scratch/t
rmdir tmp-scratch
"#;

/// The script did what its comments say, read from the model of the mount.
fn every_kind_happened(expected: &Flat) {
    let top = file(expected, "made/top.txt");
    assert_eq!(top.len(), 9_004);
    assert_eq!(&top[..15], b"created at top\n");
    assert!(top[15..9_000].iter().all(|byte| *byte == 0), "hole");
    assert_eq!(&top[9_000..], b"tail");
    let mut app = pattern(7, 300_017);
    app[123_456..123_467].copy_from_slice(b"OVERWRITTEN");
    same_bytes(file(expected, "target/debug/app"), &app, "overwritten app");
    let guide = file(expected, "docs/guide.md");
    assert_eq!(guide.len(), 70_000);
    assert_eq!(&guide[..5_000], pattern(10, 5_000));
    assert!(guide[5_000..].iter().all(|byte| *byte == 0), "hole");
    same_bytes(
        file(expected, "target/release/app"),
        &[b'R'; 150_000],
        "new app",
    );
    assert_eq!(file(expected, ".git/HEAD"), b"ref: refs/heads/topic\n");
    assert_eq!(file(expected, ".git/index").len(), 4_321 + 10);
    same_bytes(
        file(expected, ".cache/tool/data.bin"),
        &pattern(6, 1_000),
        "truncated cache file",
    );
    assert_eq!(file(expected, "dist/bundle.js"), b"");
    same_bytes(
        file(expected, "made/out-copy.o"),
        &pattern(8, 9_000),
        "copied object",
    );
    let readme = at(expected, "README.md");
    assert_eq!(readme.payload, b"# base\nappended line\n");
    assert_eq!(readme.links, 2);
    assert_eq!(at(expected, "docs/README-alias.md"), readme);
    let old = at(expected, "src/lib-alias.rs");
    assert_eq!(
        (old.payload.as_slice(), old.links, old.mode),
        (b"pub fn lib() {}\n".as_slice(), 1, 0o600)
    );
    let new = at(expected, "src/lib.rs");
    assert_eq!(
        (new.payload.as_slice(), new.links, new.mode),
        (b"pub fn replaced() {}\n".as_slice(), 1, 0o644)
    );
    assert_eq!(at(expected, ".git/objects/aa/bbccdd").links, 2);
    assert_eq!(at(expected, "dist/app-link").links, 2);
    assert_eq!(at(expected, "made/zero-alias").links, 2);
    assert_eq!(at(expected, "dist/assets/logo.svg").kind, DIRECTORY);
    assert_eq!(at(expected, "empty-dir").mode, 0o555);
    assert_eq!(at(expected, "docs").mode, 0o700);
    let dangling = at(expected, "deep-link");
    assert_eq!(
        (dangling.kind, dangling.payload.as_slice()),
        (SYMLINK, b"src/deep".as_slice())
    );
    assert_eq!(at(expected, "made/dir-link").payload, b"sub");
    for (path, time) in [
        (
            "node_modules/pkg/package.json",
            (1_000_000_000, 123_456_789),
        ),
        ("docs/deep-moved/a", (1_073_741_824, 0)),
        ("made/sub/empty", (1_286_705_410, 1)),
        ("target/debug/build", (946_684_799, 0)),
    ] {
        assert_eq!(at(expected, path).mtime, time, "{path}");
    }
    assert_eq!(
        file(expected, "docs/deep-moved/a/b/c/leaf-renamed.txt"),
        b"leaf\n"
    );
    absent(
        expected,
        &[
            "__pycache__",
            "tmp-scratch",
            "spare/gone.txt",
            "spare/inner",
            "readme-link",
            "src/deep",
            "node_modules/pkg/lib",
            "ignored/secret.env",
            ".cache/tool/object-alias",
            ".git/HEAD.lock",
            "made/sub/leaf",
        ],
    );
}

/// R5-1.
#[test]
fn r5_1_external_bash_changes_of_every_kind_are_committed_and_read_back_on_a_fresh_mount() {
    let rig = Rig::new("r5-1");
    let first = rig.mount(1);
    let mount = root(&first);
    let base_root = rig.harness.status(first.token).binding.effective_root;
    assert_same(&rig.base, &native(mount), "the mounted base");

    // The same script, by the same unregistered identity: on the mount, and
    // on a native directory the same builder filled.
    passed(&bash_in(COMMAND, mount, EVERY_KIND), "script on the mount");
    let expected = native(mount);
    every_kind_happened(&expected);
    let replay = rig.replay();
    passed(&bash_in(COMMAND, &replay, EVERY_KIND), "script on the copy");
    let replayed = native(&replay);
    // Kind, mode, names per inode, bytes or target and alias classes of every
    // path, and every explicit time exactly; a time the clock chose must be
    // one the clock chose on both sides.
    assert_same(
        &clock_masked(&replayed, rig.started),
        &clock_masked(&expected, rig.started),
        "the mount against the native replay",
    );
    assert_eq!(
        clocked(&replayed, rig.started),
        clocked(&expected, rig.started)
    );
    println!(
        "R5-1 model paths={} base_paths={} clock_times={} explicit_times={} replay_paths={}",
        expected.len(),
        rig.base.len(),
        clocked(&expected, rig.started),
        expected.len() - clocked(&expected, rig.started),
        replayed.len()
    );

    let before = kernel(mount);
    let record = rig.committed(first.token, "R5-1");
    assert_ne!(record.root, base_root);
    let published = rig.published(record.root);
    assert_same(&expected, &published, "the published root, fresh bind");
    assert_same(&expected, &native(mount), "the same mount after install");
    let after = kernel(mount);
    let moved: Vec<String> = before
        .iter()
        .filter(|(path, stat)| after.get(*path) != Some(stat))
        .map(|(path, stat)| format!("{path:?}: {stat:?} -> {:?}", after.get(path)))
        .collect();
    rig.unmount(&first);

    // A new Workspace of the Branch, mounted afresh.
    let fresh = rig.mount(2);
    assert_eq!(
        rig.harness.status(fresh.token).binding.effective_root,
        record.root
    );
    let again = native(root(&fresh));
    assert_same(&expected, &again, "the fresh mount");
    let table = kernel(root(&fresh));
    assert_kernel(&expected, &table, "R5-1 fresh mount");
    let same_inodes = table
        .iter()
        .filter(|(path, stat)| after.get(*path).is_some_and(|old| old.ino == stat.ino))
        .count();
    println!(
        "R5-1 oracles published_paths={} fresh_mount_paths={} kernel_paths={} same_mount_stat_changes_at_install={} fresh_mount_same_st_ino={same_inodes}/{}",
        published.len(),
        again.len(),
        table.len(),
        moved.len(),
        table.len()
    );
    rig.unmount(&fresh);
    rig.finish();
    // Install changed no kernel-visible attribute of the first mount.
    assert_eq!(before.len(), after.len());
    assert!(
        moved.is_empty(),
        "{} of {} paths changed at install: {:#?}",
        moved.len(),
        before.len(),
        &moved[..moved.len().min(8)]
    );
}

/// R5-2, the mounted part. The stale unchanged candidate over a moved head
/// is proven with two Workspaces elsewhere.
#[test]
fn r5_2_unchanged_is_up_to_date_and_a_change_is_committed_once() {
    let rig = Rig::new("r5-2");
    let ready = rig.mount(1);
    let mount = root(&ready);
    let bound = rig.harness.status(ready.token).binding;
    let history = rig.history();

    // Every name, attribute and byte read, and a command that changes nothing.
    let base = native(mount);
    passed(
        &bash_in(
            COMMAND,
            mount,
            "set -e; ls -lR . >/dev/null; cat README.md .git/index target/debug/app >/dev/null; test -d .git",
        ),
        "read-only command",
    );
    let unchanged = rig.up_to_date(ready.token, "R5-2 unchanged");
    assert_eq!(
        unchanged,
        (bound.branch.head_commit, bound.effective_root),
        "the same head and root"
    );
    let page = rig.history();
    assert_eq!(page.records, history.records, "no new Commit record");
    assert_eq!(page.continuation, history.continuation);
    let status = rig.harness.status(ready.token);
    assert_eq!(status.activity, Activity::Idle);
    assert_eq!(
        (
            status.binding.branch.head_commit,
            status.binding.effective_root
        ),
        (bound.branch.head_commit, bound.effective_root)
    );
    assert_same(&base, &native(mount), "the mount after UpToDate");

    // Changed.
    passed(
        &bash_in(COMMAND, mount, "printf 'changed\\n' >> README.md"),
        "change",
    );
    let expected = native(mount);
    assert_eq!(file(&expected, "README.md"), b"# base\nchanged\n");
    let record = rig.committed(ready.token, "R5-2 changed");
    assert_eq!(record.parent, bound.branch.head_commit);
    assert_ne!(record.root, bound.effective_root);
    let grown = rig.history();
    let mut wanted = vec![record.clone()];
    wanted.extend(history.records.iter().cloned());
    assert_eq!(grown.records, wanted, "exactly one new record, at the head");

    // Immediately again.
    let again = rig.up_to_date(ready.token, "R5-2 again");
    assert_eq!(again, (Some(record.id), record.root));
    let page = rig.history();
    assert_eq!(page.records, grown.records, "no new Commit record");
    assert_eq!(page.continuation, grown.continuation);
    let status = rig.harness.status(ready.token);
    assert_eq!(status.activity, Activity::Idle);
    assert_eq!(
        (
            status.binding.branch.head_commit,
            status.binding.effective_root
        ),
        (Some(record.id), record.root)
    );
    assert_same(&expected, &rig.published(record.root), "the published root");
    println!(
        "R5-2 unchanged=UpToDate head={:?} history_before={} changed=Committed history_after={} again=UpToDate paths={}",
        bound.branch.head_commit,
        history.records.len(),
        grown.records.len(),
        expected.len()
    );
    rig.unmount(&ready);
    rig.finish();
}

/// R5-3.
#[test]
fn r5_3_commit_captures_what_every_launch_published_whatever_became_of_the_process() {
    let rig = Rig::new("r5-3");
    let ready = rig.mount(1);
    let mount = root(&ready);

    // Launch A exits 0.
    let a = bash_in(
        COMMAND,
        mount,
        "set -e; printf 'from launch A\\n' > a.txt; mkdir .git/a; printf 'A in git\\n' > .git/a/note",
    );
    passed(&a, "launch A");
    // Launch B writes, renames and exits 3.
    let b = bash_in(
        COMMAND,
        mount,
        "printf 'from launch B\\n' > b.txt; printf 'B appended\\n' >> README.md; mv -T src/main.rs src/main-b.rs; exit 3",
    );
    assert_eq!(b.status.code(), Some(3), "{b:?}");
    // Launch C has written, and is alive with an open descriptor and its
    // working directory inside the mount when the Commit runs.
    let home = mount.join(".cache/tool");
    let log = home.join("held.log");
    let mut c = Background::spawn(
        COMMAND,
        &home,
        "exec 3>>held.log; printf 'C before Commit\\n' >&3; printf 'C other\\n' > ../c.txt; read -r _; printf 'C after Commit\\n' >&3",
    );
    until("launch C published its writes", || {
        fs::read(&log).is_ok_and(|bytes| bytes == b"C before Commit\n")
            && fs::read(mount.join(".cache/c.txt")).is_ok_and(|bytes| bytes == b"C other\n")
    });
    assert!(c.alive());
    // Seen by a process of the same identity: the test's own identity may
    // not inspect another user's process.
    let held = bash(
        COMMAND,
        &format!("readlink /proc/{0}/fd/3 /proc/{0}/cwd", c.pid()),
    );
    passed(&held, "descriptor and working directory of launch C");
    assert_eq!(
        String::from_utf8(held.stdout).unwrap(),
        format!("{}\n{}\n", log.display(), home.display())
    );

    let expected = native(mount);
    assert_eq!(file(&expected, "a.txt"), b"from launch A\n");
    assert_eq!(file(&expected, ".git/a/note"), b"A in git\n");
    assert_eq!(file(&expected, "b.txt"), b"from launch B\n");
    assert_eq!(file(&expected, "README.md"), b"# base\nB appended\n");
    assert_eq!(file(&expected, "src/main-b.rs"), b"fn main() {}\n");
    absent(&expected, &["src/main.rs"]);
    assert_eq!(
        file(&expected, ".cache/tool/held.log"),
        b"C before Commit\n"
    );
    assert_eq!(file(&expected, ".cache/c.txt"), b"C other\n");

    let record = rig.committed(ready.token, "R5-3");
    assert!(c.alive(), "launch C outlived the Commit");
    let published = rig.published(record.root);
    assert_same(&expected, &published, "the published root, fresh bind");
    println!(
        "R5-3 launches=3 exit_codes=0,3,alive registered=0 background_pid={} paths={}",
        c.pid(),
        published.len()
    );

    // C goes on writing through its descriptor after the Commit and exits.
    // That later write is an active mutation; the next Commit captures it.
    let status = c.release();
    assert!(status.success(), "{status:?}");
    assert_eq!(
        fs::read(&log).unwrap(),
        b"C before Commit\nC after Commit\n"
    );
    let later = native(mount);
    let second = rig.committed(ready.token, "R5-3 later write");
    assert_eq!(second.parent, Some(record.id));
    assert_same(&later, &rig.published(second.root), "the second root");
    rig.unmount(&ready);
    let fresh = rig.mount(2);
    assert_same(&later, &native(root(&fresh)), "the fresh mount");
    assert_kernel(&later, &kernel(root(&fresh)), "R5-3 fresh mount");
    rig.unmount(&fresh);
    rig.finish();
}

/// Five rounds on one mount. Later rounds change and remove what earlier
/// rounds made, and what the base held.
const ROUNDS: [&str; 5] = [
    // Create.
    r#"set -euo pipefail; umask 022
mkdir -p work/a/b .git/objects/r1 target/r1 .cache/r1
printf 'round one\n' > work/one.txt
head -c 200000 /dev/zero | tr '\0' 'q' > work/a/big.bin
printf 'obj r1' > .git/objects/r1/obj
printf 'out r1' > target/r1/out
printf 'cache r1' > .cache/r1/entry
ln -s one.txt work/link-one
ln work/one.txt work/a/b/one-alias
"#,
    // Change content and metadata of round-one files and of base files.
    r#"set -euo pipefail; umask 022
printf 'appended in round two\n' >> work/one.txt
printf 'MIDDLE' | dd of=work/a/big.bin bs=1 seek=100000 conv=notrunc status=none
truncate -s 150000 work/a/big.bin
chmod 0600 work/a/big.bin
mv -T work/a/b work/b-moved
printf 'more' >> .git/index
printf 'PATCH' | dd of=target/debug/app bs=1 seek=250000 conv=notrunc status=none
touch -d '2005-05-05 05:05:05.5 UTC' .cache/r1/entry
"#,
    // Remove round-one files and base files; replace and recreate names.
    r#"set -euo pipefail; umask 022
rm -f work/one.txt work/link-one .git/objects/r1/obj
rmdir .git/objects/r1
rm -rf spare
rm -f node_modules/.bin/pkg dist/bundle.js
printf 'replacement\n' > src/main.rs.tmp
mv -T src/main.rs.tmp src/main.rs
rm -f debug.log
printf 'recreated\n' > debug.log
"#,
    // Metadata only, and renames of populated directories.
    r#"set -euo pipefail; umask 022
chmod 0700 work
chmod 0444 work/a/big.bin
touch -d '2011-11-11 11:11:11.111111111 UTC' work/a README.md
mv -T node_modules/pkg vendor-pkg
mv -T docs/guide.md work/guide.md
"#,
    // Holes, truncation to nothing, alias and subtree removal, a file
    // replaced by a symlink.
    r#"set -euo pipefail; umask 022
truncate -s 500000 target/debug/app
: > .git/index
rm -f ignored/object-alias-2
rm -rf work/b-moved __pycache__
ln -s ../README.md work/readme-link
mkdir -p work/a/new
printf 'five\n' > work/a/new/five.txt
rm -f vendor-pkg/index.js
ln -s package.json vendor-pkg/index.js
"#,
];

/// R5-8.
#[test]
fn r5_8_five_incremental_commits_on_one_mount_each_read_back_completely() {
    let rig = Rig::new("r5-8");
    let ready = rig.mount(1);
    let mount = root(&ready);
    let mut parent = rig.harness.status(ready.token).binding.branch.head_commit;
    let mut expected = native(mount);
    let mut records = Vec::new();
    for (round, script) in ROUNDS.iter().enumerate() {
        let what = format!("R5-8 round {}", round + 1);
        passed(&bash_in(COMMAND, mount, script), &what);
        let changed = native(mount);
        assert!(changed != expected, "{what}: the round changed nothing");
        expected = changed;
        let record = rig.committed(ready.token, &what);
        assert_eq!(record.parent, parent, "{what}: parent is the previous head");
        let published = rig.published(record.root);
        assert_same(&expected, &published, &format!("{what}: published root"));
        // The mount is still mounted and serves the same tree after install.
        assert_same(&expected, &native(mount), &format!("{what}: same mount"));
        println!(
            "{what}: paths={} oracles=published,same_mount",
            published.len()
        );
        parent = Some(record.id);
        records.push(record);
    }
    // What the rounds left, read from the last model.
    let mut app = pattern(7, 300_017);
    app[250_000..250_005].copy_from_slice(b"PATCH");
    app.resize(500_000, 0);
    same_bytes(
        file(&expected, "target/debug/app"),
        &app,
        "patched and extended app",
    );
    let mut big = vec![b'q'; 150_000];
    big[100_000..100_006].copy_from_slice(b"MIDDLE");
    same_bytes(file(&expected, "work/a/big.bin"), &big, "round-one file");
    assert_eq!(at(&expected, "work/a/big.bin").mode, 0o444);
    assert_eq!(file(&expected, ".git/index"), b"");
    assert_eq!(
        at(&expected, "README.md").mtime,
        (1_321_009_871, 111_111_111)
    );
    assert_eq!(
        at(&expected, ".cache/r1/entry").mtime,
        (1_115_269_505, 500_000_000)
    );
    assert_eq!(at(&expected, "vendor-pkg/index.js").kind, SYMLINK);
    assert_eq!(at(&expected, ".git/objects/aa/bbccdd").links, 2);
    absent(
        &expected,
        &[
            "work/one.txt",
            "work/b-moved",
            "spare",
            "node_modules/pkg",
            "__pycache__",
            ".git/objects/r1",
            "dist/bundle.js",
        ],
    );
    let page = rig.history();
    let newest: Vec<_> = page
        .records
        .iter()
        .take(5)
        .map(|record| record.id)
        .collect();
    let made: Vec<_> = records.iter().rev().map(|record| record.id).collect();
    assert_eq!(newest, made, "five records, newest first");
    let last = records.last().unwrap().root;
    rig.unmount(&ready);

    let fresh = rig.mount(2);
    assert_eq!(rig.harness.status(fresh.token).binding.effective_root, last);
    assert_same(&expected, &native(root(&fresh)), "the fresh mount");
    assert_kernel(&expected, &kernel(root(&fresh)), "R5-8 fresh mount");
    println!(
        "R5-8 commits={} history_records={} final_paths={}",
        records.len(),
        page.records.len(),
        expected.len()
    );
    rig.unmount(&fresh);
    rig.finish();
}

/// Not a proof by itself. Re-executed by the mapping case as a separate
/// process of the command identity that nothing registered: it maps the file
/// shared, closes its descriptor, stores, and exits with the page dirty.
#[test]
fn map_close_store_exit_process() {
    let Some(path) = std::env::var_os(MAP_STORE) else {
        return;
    };
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    let map = Mapping::shared(&file, 2 * PAGE);
    drop(file);
    map.store(PAGE + 5, b"exit-store");
    unsafe { libc::_exit(0) }
}

/// R5-9. Commit includes exactly the published frontier: a store through a
/// shared mapping is in it once the kernel has written the page back, and
/// nothing here claims an unsynchronized store is.
#[test]
fn r5_9_stores_through_shared_mappings_are_committed_once_written_back() {
    let rig = Rig::new("r5-9");
    let ready = rig.mount(1);
    let mount = root(&ready);

    // (a) A base file mapped by this process. The descriptor and the mapping
    // stay live across the Commit.
    let app = mount.join("target/debug/app");
    let mut content = pattern(7, 300_017);
    same_bytes(&fs::read(&app).unwrap(), &content, "base app");
    let handle = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&app)
        .unwrap();
    let length = 64 * PAGE;
    let map = Mapping::shared(&handle, length);
    same_bytes(&map.load(0, length), &content[..length], "mapped read");
    for (offset, bytes) in [
        (3 * PAGE + 17, b"synchronized-store".as_slice()),
        (40 * PAGE - 4, b"across-a-page-boundary".as_slice()),
    ] {
        map.store(offset, bytes);
        content[offset..offset + bytes.len()].copy_from_slice(bytes);
    }
    map.sync();
    same_bytes(&direct(&app), &content, "daemon read after msync");
    let expected = native(mount);
    same_bytes(file(&expected, "target/debug/app"), &content, "model");
    let first = rig.committed(ready.token, "R5-9 msync");
    let published = rig.published(first.root);
    assert_same(&expected, &published, "the published root after msync");
    same_bytes(
        file(&published, "target/debug/app"),
        &content,
        "published bytes after msync",
    );
    same_bytes(
        &map.load(0, length),
        &content[..length],
        "mapping after install",
    );

    // The same mapping, after install: another synchronized store.
    map.store(10 * PAGE, b"store-after-install");
    content[10 * PAGE..10 * PAGE + 19].copy_from_slice(b"store-after-install");
    map.sync();
    same_bytes(&direct(&app), &content, "daemon read of the later store");
    map.unmap();
    drop(handle);

    // (b) A file made on the mount, then mapped by a child that closes its
    // descriptor, stores and exits.
    let mapped = mount.join("src/mapped.bin");
    passed(
        &bash_in(
            COMMAND,
            mount,
            "set -euo pipefail; head -c 8292 /dev/zero | tr '\\0' 'm' > src/mapped.bin",
        ),
        "file for the child",
    );
    let mut stored = vec![b'm'; 2 * PAGE + 100];
    same_bytes(&fs::read(&mapped).unwrap(), &stored, "file for the child");
    let exited = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "map_close_store_exit_process", "--nocapture"])
        .env(MAP_STORE, &mapped)
        .uid(COMMAND)
        .gid(COMMAND)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(exited.status.success(), "{exited:?}");
    stored[PAGE + 5..PAGE + 15].copy_from_slice(b"exit-store");
    // Bounded wait until the daemon serves the store: every byte of this
    // read is a READ answered from published state.
    until("the daemon has the exited child's store", || {
        direct(&mapped) == stored
    });
    let expected = native(mount);
    same_bytes(file(&expected, "src/mapped.bin"), &stored, "model");
    same_bytes(file(&expected, "target/debug/app"), &content, "model");
    let second = rig.committed(ready.token, "R5-9 child store");
    assert_eq!(second.parent, Some(first.id));
    let published = rig.published(second.root);
    assert_same(&expected, &published, "the published root after the child");
    same_bytes(
        file(&published, "src/mapped.bin"),
        &stored,
        "published bytes of the child's store",
    );
    same_bytes(
        file(&published, "target/debug/app"),
        &content,
        "published bytes of the later store",
    );
    let drained = rig.unmount(&ready);
    let stores = counter(&drained, "store_units");
    // Page 3, pages 39 and 40, page 10 and the child's page each arrived as
    // a WRITE carrying the page-cache flag.
    assert!(stores >= 4, "{drained}");

    let fresh = rig.mount(2);
    let again = root(&fresh);
    for (path, bytes) in [("target/debug/app", &content), ("src/mapped.bin", &stored)] {
        same_bytes(&fs::read(again.join(path)).unwrap(), bytes, path);
        same_bytes(&direct(&again.join(path)), bytes, path);
    }
    assert_same(&expected, &native(again), "the fresh mount");
    println!(
        "R5-9 store_units={stores} msync_bytes={} child_bytes={} paths={}",
        content.len(),
        stored.len(),
        expected.len()
    );
    rig.unmount(&fresh);
    rig.finish();
}

/// The rig's own claim that a replay starts from the mounted base.
#[test]
fn the_native_replay_starts_from_the_same_tree_as_the_mounted_base() {
    let rig = Rig::new("r5-rig");
    let replay = rig.replay();
    assert_same(&rig.base, &native(&replay), "a replay before any script");
    let table = kernel(&replay);
    assert!(table
        .values()
        .all(|stat| (stat.uid, stat.gid) == (COMMAND, COMMAND)));
    assert_eq!(
        clocked(&rig.base, rig.started),
        0,
        "every base time is explicit"
    );
    println!(
        "R5-RIG base_paths={} replay_paths={}",
        rig.base.len(),
        table.len()
    );
    rig.finish();
}

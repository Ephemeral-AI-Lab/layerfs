//! A merged install changes what a held cwd names, and the mount serves on.
//!
//! Each case holds a directory of Workspace A as a process's cwd. Another
//! Workspace publishes a change to that directory, and A's next Commit merges
//! it and installs the merged root. No path walk refreshes the held cwd: the
//! kernel still names it by its old inode. The held process makes the checked
//! requests itself, from that cwd, once it is released.
//!
//! - The merge drops the directory's serial: GETATTR, OPENDIR and LOOKUP
//!   through the cwd answer ESTALE for each request alone, nothing is retained
//!   and the mount keeps serving (review finding #7).
//! - The merge moves the directory under another one: a rename of that other
//!   directory into the cwd is refused instead of making a cycle cut off from
//!   the root, and the kernel parent index follows the next LOOKUP (#17).
#![cfg(target_os = "linux")]
use crate::mounted;
use crate::rig;
use layerfs_protocol::daemon::{ReadyMount, Reply};
use mounted::COMMAND;
use rig::{bash_in, passed, root, Background, Rig};

/// A's Commit merges with the other Workspace's and installs the result.
fn merged(rig: &Rig, a: &ReadyMount) {
    passed(
        &bash_in(COMMAND, root(a), "printf alpha > from-a"),
        "A changes another file",
    );
    let done = rig.commit(a.token).expect("A commits");
    assert!(
        matches!(done.reply, Reply::MergeCommitted(_)),
        "A's Commit merges: {:?}",
        done.reply
    );
}
/// No request of the mount was retained: it is not terminal.
fn serving(rig: &Rig, a: &ReadyMount, what: &str) {
    let work = rig.harness.status(a.token).kernel.unwrap().work.unwrap();
    assert_eq!(work.retained, 0, "{what}: {work:?}");
}
/// The held process waits for its release, then runs `checks` in its cwd;
/// a failed check is reported on its standard error.
fn held(directory: &std::path::Path, checks: &str) -> Background {
    let script = format!("read line; set -u\n{checks}");
    Background::spawn(COMMAND, directory, &script)
}

#[test]
fn a_held_directory_the_merge_dropped_answers_estale_and_the_mount_serves_on() {
    let rig = Rig::new("stale-dropped-serial");
    let a = rig.mount(1);
    passed(
        &bash_in(
            COMMAND,
            root(&a),
            "mkdir -p doomed/inner && printf x > doomed/f",
        ),
        "A makes doomed",
    );
    rig.committed(a.token, "A publishes doomed");
    let b = rig.mount(2);
    // GETATTR, OPENDIR and LOOKUP of the dropped directory, from inside it.
    let process = held(
        &root(&a).join("doomed"),
        "stale() { case $1 in *'Stale file handle'*) ;; *) echo \"$2: $1\" >&2; exit 1;; esac; }
        out=$(stat . 2>&1) && { echo \"stat passed: $out\" >&2; exit 1; }; stale \"$out\" stat
        out=$(ls . 2>&1) && { echo \"ls passed: $out\" >&2; exit 1; }; stale \"$out\" ls
        out=$(stat f 2>&1) && { echo \"lookup passed: $out\" >&2; exit 1; }; stale \"$out\" lookup
        exit 0",
    );
    passed(
        &bash_in(COMMAND, root(&b), "rm -rf doomed"),
        "B removes doomed",
    );
    rig.committed(b.token, "B publishes the removal");
    merged(&rig, &a);
    let status = process.release();
    assert!(status.success(), "the held cwd was not stale: {status:?}");
    serving(&rig, &a, "after the stale requests");
    passed(
        &bash_in(
            COMMAND,
            root(&a),
            "test \"$(cat from-a)\" = alpha && test ! -e doomed \
             && printf beta > after && test \"$(cat after)\" = beta",
        ),
        "the mount serves on",
    );
    serving(&rig, &a, "after serving on");
    rig.unmount(&a);
    rig.unmount(&b);
    rig.finish();
}

#[test]
fn a_rename_into_a_held_directory_the_merge_moved_makes_no_cycle() {
    let rig = Rig::new("stale-parent-index");
    let a = rig.mount(1);
    passed(&bash_in(COMMAND, root(&a), "mkdir x y"), "A makes x and y");
    rig.committed(a.token, "A publishes x and y");
    let b = rig.mount(2);
    // A's kernel still holds y as the cwd it looked up at the root. Moving x
    // into it would make x and y a cycle cut off from the root. The first
    // attempt runs before any LOOKUP of y's new place, so only the checked
    // parent index can refuse it; after `test -d x/y` it is a known cycle.
    let mount = root(&a).display().to_string();
    let process = held(
        &root(&a).join("y"),
        &format!(
            "m={mount}
            refused() {{ case $1 in *'Invalid argument'*|*'subdirectory of itself'*) ;; *) echo \"$2: $1\" >&2; exit 1;; esac; }}
            out=$(mv -T $m/x x2 2>&1) && {{ echo 'stale index: x moved into y' >&2; exit 1; }}; refused \"$out\" stale
            test -d $m/x && test ! -e x2 || {{ echo 'x moved' >&2; exit 1; }}
            test -d $m/x/y || {{ echo 'y is not under x' >&2; exit 1; }}
            out=$(mv -T $m/x x2 2>&1) && {{ echo 'cycle: x moved into y' >&2; exit 1; }}; refused \"$out\" cycle
            mkdir $m/z && mv -T $m/z z && test -d $m/x/y/z || {{ echo 'z did not move in' >&2; exit 1; }}
            exit 0"
        ),
    );
    passed(&bash_in(COMMAND, root(&b), "mv y x/y"), "B moves y under x");
    rig.committed(b.token, "B publishes the move");
    merged(&rig, &a);
    let status = process.release();
    assert!(status.success(), "a rename made a cycle: {status:?}");
    serving(&rig, &a, "after the refused renames");
    passed(
        &bash_in(
            COMMAND,
            root(&a),
            "test \"$(cat from-a)\" = alpha && test -d x/y/z && test ! -e y",
        ),
        "the mount serves on",
    );
    rig.unmount(&a);
    rig.unmount(&b);
    rig.finish();
}

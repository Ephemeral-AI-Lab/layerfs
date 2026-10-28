//! A missing name is answered as a negative entry (#359 R4): the kernel asks
//! once per entry lifetime instead of once per probe. Every namespace change
//! through the mount makes the name visible at once, and a merged install
//! makes a name another Workspace added visible after its Commit returns.
#![cfg(target_os = "linux")]
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
#[path = "support/mounted_commit.rs"]
mod rig;
use layerfs_bridge::control::Reply;
use layerfs_fuse::request::Opcode;
use mounted::COMMAND;
use rig::{bash_in, passed, root, Rig};

/// The opcode counts in a drain receipt's `OpcodeWork`.
fn opcodes(receipt: &str) -> Vec<u64> {
    let key = "OpcodeWork { opcodes: [";
    let at = receipt.find(key).unwrap_or_else(|| panic!("{receipt}")) + key.len();
    let end = at + receipt[at..].find(']').unwrap();
    receipt[at..end]
        .split(", ")
        .map(|count| count.parse().unwrap())
        .collect()
}

#[test]
fn a_missing_name_is_asked_once_and_every_namespace_change_shows_at_once() {
    let rig = Rig::new("issue359-negative-entries");
    let a = rig.mount(1);
    // 200 probes of one missing name, then each kind of change that binds a
    // name the kernel holds as missing; each must be visible immediately.
    let script = "set -e
        for i in $(seq 200); do ! stat absent-name 2>/dev/null; done
        ! stat new-file 2>/dev/null; printf one > new-file; test \"$(cat new-file)\" = one
        ! stat new-dir 2>/dev/null; mkdir new-dir; test -d new-dir
        ! stat new-dir/inner 2>/dev/null; printf two > new-dir/inner; test -f new-dir/inner
        ! stat new-link 2>/dev/null; ln -s new-file new-link; test \"$(cat new-link)\" = one
        ! stat new-hard 2>/dev/null; ln new-file new-hard; test \"$(stat -c %h new-file)\" = 2
        ! stat moved 2>/dev/null; mv new-hard moved; test -f moved; ! stat new-hard 2>/dev/null
        ! stat via-rename-dir 2>/dev/null; mkdir -p elsewhere; printf three > elsewhere/via-rename-dir
        mv elsewhere/via-rename-dir .; test \"$(cat via-rename-dir)\" = three
        rm new-file; ! stat new-file 2>/dev/null; printf four > new-file; test \"$(cat new-file)\" = four
        ! stat absent-name 2>/dev/null";
    passed(&bash_in(COMMAND, root(&a), script), "probes and changes");
    let receipt = rig.unmount(&a);
    let counts = opcodes(&receipt);
    let lookups = counts[Opcode::Lookup as usize];
    println!("NEGATIVE_ENTRIES lookups={lookups} receipt={receipt}");
    // ENOENT answers would be re-asked: at least the 201 probes of one name.
    assert!(
        lookups < 60,
        "repeated misses reached the daemon: {lookups}"
    );
    rig.finish();
}

#[test]
fn a_name_another_workspace_added_shows_after_the_merged_install() {
    let rig = Rig::new("issue359-negative-merge");
    let a = rig.mount(1);
    let b = rig.mount(2);
    // A caches the name as missing; B adds it and publishes first.
    passed(
        &bash_in(
            COMMAND,
            root(&a),
            "! stat from-b 2>/dev/null && ! stat from-b 2>/dev/null",
        ),
        "A sees from-b missing",
    );
    passed(
        &bash_in(COMMAND, root(&b), "printf bravo > from-b"),
        "B creates",
    );
    rig.committed(b.token, "B publishes from-b");
    // A has its own change, so its Commit merges with B's and installs the
    // merged root; the kernel's negative entry must not outlive that.
    passed(
        &bash_in(COMMAND, root(&a), "printf alpha > from-a"),
        "A creates",
    );
    let done = rig.commit(a.token).expect("A commits");
    assert!(
        matches!(done.reply, Reply::MergeCommitted(_)),
        "A's Commit merges: {:?}",
        done.reply
    );
    passed(
        &bash_in(
            COMMAND,
            root(&a),
            "test \"$(cat from-b)\" = bravo && test \"$(cat from-a)\" = alpha",
        ),
        "A sees from-b after the merged install",
    );
    rig.unmount(&a);
    rig.unmount(&b);
    rig.finish();
}

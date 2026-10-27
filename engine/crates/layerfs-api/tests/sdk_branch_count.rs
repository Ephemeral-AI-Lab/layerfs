//! A Branch is found by its name without reading the project's other Branches
//! (#352 range 002: fork's rows grew with the Branches the replay had made).
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use std::fs;
use support::Fixture;

#[test]
fn fork_and_branch_reads_are_independent_of_branch_count() {
    let fixture = Fixture::new("sdk-branch-count", |source| {
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        1
    });
    let mut made = 0;
    let mut observed = Vec::new();
    for count in [10usize, 1000] {
        while made < count {
            fixture.fork(&format!("lane-{made:05}"));
            made += 1;
        }
        let before = fixture.work();
        fixture.fork(&format!("probe-{count:05}"));
        let fork = fixture.since(before);
        let stat = || {
            fixture.call(
                "fs.stat",
                vec![
                    ("branch", SdkValue::text("lane-00005")),
                    ("path", SdkValue::text("/a.txt")),
                ],
            )
        };
        // The same call first, so both counts measure the same warm state.
        stat();
        let before = fixture.work();
        let stat = stat();
        let read = fixture.since(before);
        assert_eq!(stat.get_text("type"), Some("file"));
        println!("SDK_BRANCH_COUNT branches={count} fork={fork:?} stat={read:?} profile=Disposable cache=warm-repeat diagnostic_count_gate=true");
        observed.push((count, fork, read));
    }
    let (_, fork_few, read_few) = observed[0];
    let (_, fork_many, read_many) = observed[1];
    assert_eq!(
        (fork_few.statements, fork_few.rows),
        (fork_many.statements, fork_many.rows),
        "fork reads every Branch: {observed:?}"
    );
    assert_eq!(
        (read_few.statements, read_few.rows),
        (read_many.statements, read_many.rows),
        "a read on a named Branch reads every Branch: {observed:?}"
    );
    fixture.cleanup();
}

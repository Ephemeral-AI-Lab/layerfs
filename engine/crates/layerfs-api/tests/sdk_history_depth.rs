//! Merge and publish work follows the Commits since the merge base, not the
//! whole history (#352 range 002: a two-file merge's SQL statements grew with
//! history depth; #353 removed the walks' 4,096-row bound).
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use layerfs_history::{BranchId, HistoryName, LayerStackId, NodeId};
use std::fs;
use support::{operation, write, Fixture, Work};

/// The installed fixture's stack.
const STACK: [u8; 16] = [23; 16];

fn head(fixture: &Fixture, name: &str) -> NodeId {
    let stack = LayerStackId::from_authority(STACK);
    let id = BranchId::named(stack, &HistoryName::new(name).unwrap());
    let branch = fixture.service.store.history().branch(id).unwrap().unwrap();
    NodeId::Commit(branch.head_commit.unwrap())
}

/// The catalog's own ancestry work for two heads: both reachability checks
/// and the merge base, as the SDK merge asks them.
fn ancestry(fixture: &Fixture, a: &str, b: &str) -> (Work, Option<NodeId>) {
    let (a, b) = (head(fixture, a), head(fixture, b));
    let history = fixture.service.store.history();
    let before = fixture.work();
    assert!(!history.is_ancestor(b, a).unwrap());
    assert!(!history.is_ancestor(a, b).unwrap());
    let base = history.merge_base(a, b).unwrap();
    (fixture.since(before), base)
}

fn text(value: &str) -> SdkValue {
    SdkValue::text(value)
}

/// One published round: two lanes from main, one Commit each, merged,
/// published; the publish's own work.
fn round(fixture: &Fixture, i: usize) -> Work {
    let (x, y, m) = (
        format!("x-{i:04}"),
        format!("y-{i:04}"),
        format!("m-{i:04}"),
    );
    fixture.fork(&x);
    fixture.fork(&y);
    fixture.apply(&x, vec![write("/lane/x.txt", format!("x {i}\n"))]);
    fixture.apply(&y, vec![write("/lane/y.txt", format!("y {i}\n"))]);
    let merged = fixture.call(
        "branch.merge",
        vec![
            ("branch", text(&x)),
            ("with", text(&y)),
            ("name", text(&m)),
            ("message", text("round")),
        ],
    );
    assert_eq!(merged.get_text("kind"), Some("Merged"), "{merged:?}");
    let before = fixture.work();
    let published = fixture.call("branch.publish", vec![("branch", text(&m))]);
    let publish = fixture.since(before);
    assert_eq!(published.get_text("kind"), Some("Added"), "{published:?}");
    publish
}

/// The scale probe's merge: two forks of main, two new files each. It is not
/// published, so main's tree has the same shape at every depth. Returns the
/// catalog's ancestry work and base, and the whole SDK merge's work.
fn probe(fixture: &Fixture, tag: &str) -> (Work, Option<NodeId>, Work) {
    let (a, b, m) = (
        format!("pa-{tag}"),
        format!("pb-{tag}"),
        format!("pm-{tag}"),
    );
    fixture.fork(&a);
    fixture.fork(&b);
    fixture.apply(
        &a,
        vec![
            operation("mkdir", &format!("/probe-{tag}")),
            write(&format!("/probe-{tag}/a1.txt"), "a1\n"),
            write(&format!("/probe-{tag}/a2.txt"), "a2\n"),
        ],
    );
    fixture.apply(
        &b,
        vec![
            operation("mkdir", &format!("/probe-{tag}")),
            write(&format!("/probe-{tag}/b1.txt"), "b1\n"),
            write(&format!("/probe-{tag}/b2.txt"), "b2\n"),
        ],
    );
    let (walks, base) = ancestry(fixture, &a, &b);
    let before = fixture.work();
    let merged = fixture.call(
        "branch.merge",
        vec![
            ("branch", text(&a)),
            ("with", text(&b)),
            ("name", text(&m)),
            ("message", text("probe")),
        ],
    );
    let merge = fixture.since(before);
    assert_eq!(merged.get_text("kind"), Some("Merged"), "{merged:?}");
    assert!(!merged.get_bool("fastForward"), "{merged:?}");
    (walks, base, merge)
}

#[test]
fn merge_and_publish_are_independent_of_history_depth() {
    let fixture = Fixture::new("sdk-history-depth", |source| {
        fs::create_dir(source.join("lane")).unwrap();
        fs::write(source.join("lane/x.txt"), b"x\n").unwrap();
        fs::write(source.join("lane/y.txt"), b"y\n").unwrap();
        3
    });
    let mut rounds = 0;
    let mut observed = Vec::new();
    for depth in [4usize, 60] {
        while rounds < depth {
            round(&fixture, rounds);
            rounds += 1;
        }
        // A first probe warms the caches, so both depths measure the same state.
        probe(&fixture, &format!("{:04}", depth + 1));
        let (walks, base, merge) = probe(&fixture, &format!("{depth:04}"));
        let main = fixture
            .service
            .store
            .history()
            .layer_stack(LayerStackId::from_authority(STACK))
            .unwrap()
            .unwrap()
            .head_layer;
        assert_eq!(
            base,
            Some(NodeId::Layer(main)),
            "the probe's merge base is main"
        );
        let publish = round(&fixture, rounds);
        rounds += 1;
        println!("SDK_HISTORY_DEPTH rounds={depth} ancestry={walks:?} merge={merge:?} publish={publish:?} profile=Disposable cache=warm-after-identical-probe diagnostic_count_gate=true");
        observed.push((depth, walks, merge, publish));
    }
    let (_, walks_low, merge_low, publish_low) = observed[0];
    let (_, walks_high, merge_high, publish_high) = observed[1];
    assert_eq!(
        (walks_low.statements, walks_low.rows),
        (walks_high.statements, walks_high.rows),
        "merge ancestry reads history: {observed:?}"
    );
    assert_eq!(
        (publish_low.statements, publish_low.rows),
        (publish_high.statements, publish_high.rows),
        "a publish reads history: {observed:?}"
    );
    // The merge's content work varies with delta chains (bounded by their
    // depth caps); its history work is the ancestry above.
    assert!(
        merge_high.statements <= merge_low.statements * 3 / 2,
        "a two-file merge grows with history: {observed:?}"
    );
    fixture.cleanup();
}

//! #353: the three history walks the 4,096 lineage cap used to refuse
//! (#352 F-033) succeed at 1k, 4k and 40k, and the statements a publish, a
//! merge's ancestry and a fork take stay the same as the count grows:
//! (a) Commits since a Branch's base Layer;
//! (b) Layers between two sides;
//! (c) fork from a Commit deep on its Branch's first-parent chain.
//! Long histories are built through the catalog with real roots alternating,
//! so the SDK merge and publish at the end read real trees. Disposable profile.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::{SdkCall, SdkValue};
use layerfs_content::ObjectId;
use layerfs_history::records::{
    AddLayerOutcome, AddLayerRequest, CommitMeta, CommitOutcome, CommitRequest, CommitVia,
    ForkRequest, ForkSource,
};
use layerfs_history::{BranchId, CommitId, HistoryName, LayerId, LayerStackId, NodeId};
use layerfs_store_service::operator;
use std::fmt::Write;
use std::fs;
use std::time::Instant;
use support::{write, Fixture, Work};

const STACK: [u8; 16] = [23; 16];
const COUNTS: [usize; 3] = [1_000, 4_097, 40_970];

fn stack() -> LayerStackId {
    LayerStackId::from_authority(STACK)
}

fn id(name: &str) -> BranchId {
    BranchId::named(stack(), &HistoryName::new(name).unwrap())
}

fn head(fixture: &Fixture, name: &str) -> NodeId {
    let branch = fixture
        .service
        .store
        .history()
        .branch(id(name))
        .unwrap()
        .unwrap();
    branch
        .head_commit
        .map_or(NodeId::Layer(branch.base_layer), NodeId::Commit)
}

fn hex(node: NodeId) -> String {
    node.as_slice().iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

fn text(value: &str) -> SdkValue {
    SdkValue::text(value)
}

fn fixture(label: &str) -> Fixture {
    Fixture::new(label, |source| {
        fs::create_dir(source.join("lane")).unwrap();
        fs::write(source.join("lane/seed.txt"), b"seed\n").unwrap();
        2
    })
}

fn root_of(fixture: &Fixture, node: NodeId) -> ObjectId {
    let history = fixture.service.store.history();
    match node {
        NodeId::Commit(commit) => history.commit(commit).unwrap().unwrap().root,
        NodeId::Layer(layer) => history.layer(layer).unwrap().unwrap().root,
    }
}

/// Real roots from SDK writes on a side Branch: one per content.
fn roots(fixture: &Fixture, contents: &[&str]) -> Vec<ObjectId> {
    fixture.fork("roots");
    contents
        .iter()
        .map(|content| {
            fixture.apply("roots", vec![write("/lane/roots.txt", *content)]);
            root_of(fixture, head(fixture, "roots"))
        })
        .collect()
}

fn fork(fixture: &Fixture, name: &str, source: ForkSource) {
    fixture
        .service
        .store
        .history()
        .fork(&ForkRequest {
            stack: stack(),
            branch: id(name),
            name: HistoryName::new(name).unwrap(),
            source,
        })
        .unwrap();
}

/// One catalog Commit of `root` on the Branch's head.
fn commit(fixture: &Fixture, name: &str, root: ObjectId) -> CommitId {
    let history = fixture.service.store.history();
    let branch = history.branch(id(name)).unwrap().unwrap();
    let layers = history.layer_stack(stack()).unwrap().unwrap();
    let current = head(fixture, name);
    let outcome = history
        .commit_branch(&CommitRequest {
            branch: branch.id,
            expected_head: branch.head_commit,
            expected_base: branch.base_layer,
            expected_root: root_of(fixture, current),
            candidate_root: root,
            profile: layers.profile,
            scope: layers.scope,
            second_parent: None,
            meta: CommitMeta::new(CommitVia::Files),
            force: false,
        })
        .unwrap();
    let CommitOutcome::Committed { record, .. } = outcome else {
        panic!("{name}: {outcome:?}");
    };
    record.id
}

/// `count` Commits on `name`, alternating two roots, ending at `roots[0]`.
fn commits(fixture: &Fixture, name: &str, count: usize, roots: [ObjectId; 2]) {
    for i in 1..=count {
        commit(fixture, name, roots[(count - i) % 2]);
    }
}

fn publish(fixture: &Fixture, name: &str) -> (LayerId, u64) {
    let history = fixture.service.store.history();
    let branch = history.branch(id(name)).unwrap().unwrap();
    let outcome = history
        .add_layer(&AddLayerRequest {
            stack: stack(),
            branch: branch.id,
            commit: branch.head_commit.unwrap(),
            expected_stack_head: history.layer_stack(stack()).unwrap().unwrap().head_layer,
            expected_branch_base: branch.base_layer,
        })
        .unwrap();
    let AddLayerOutcome::Added { layer } = outcome else {
        panic!("{name}: {outcome:?}");
    };
    (layer.id, layer.overwrote_count)
}

/// The SDK merge's ancestry questions for two heads, each answered once.
fn ancestry(fixture: &Fixture, a: &str, b: &str) -> (Work, Option<NodeId>) {
    let (a, b) = (head(fixture, a), head(fixture, b));
    let history = fixture.service.store.history();
    let before = fixture.work();
    assert!(!history.is_ancestor(b, a).unwrap());
    assert!(!history.is_ancestor(a, b).unwrap());
    let base = history.merge_base(a, b).unwrap();
    (fixture.since(before), base)
}

fn sdk_merge(fixture: &Fixture, branch: &str, with: &str, name: &str) -> Work {
    let before = fixture.work();
    let merged = fixture.call(
        "branch.merge",
        vec![
            ("branch", text(branch)),
            ("with", text(with)),
            ("name", text(name)),
            ("message", text("s353")),
        ],
    );
    assert_eq!(merged.get_text("kind"), Some("Merged"), "{merged:?}");
    fixture.since(before)
}

fn sdk_publish(fixture: &Fixture, branch: &str) -> (Work, SdkValue) {
    let before = fixture.work();
    let published = fixture.call("branch.publish", vec![("branch", text(branch))]);
    assert_eq!(published.get_text("kind"), Some("Added"), "{published:?}");
    (fixture.since(before), published)
}

fn flat(what: &str, works: &[(usize, Work)]) {
    let first = works[0].1.statements;
    assert!(
        works.iter().all(|(_, work)| work.statements == first),
        "{what} statements grow with the count: {works:?}"
    );
}

#[test]
fn a_commits_since_the_layer() {
    let fixture = fixture("scale-353a");
    let start = Instant::now();
    let [a, b] = <[ObjectId; 2]>::try_from(roots(&fixture, &["a\n", "b\n"])).unwrap();
    fixture.fork("other");
    fixture.apply("other", vec![write("/lane/other.txt", "other\n")]);
    let (mut walks, mut merges, mut publishes) = (Vec::new(), Vec::new(), Vec::new());
    for count in COUNTS {
        let big = format!("big{count}");
        fixture.fork(&big);
        commits(&fixture, &big, count, [a, b]);
        let (work, base) = ancestry(&fixture, &big, "other");
        assert!(matches!(base, Some(NodeId::Layer(_))), "{base:?}");
        walks.push((count, work));
        sdk_merge(&fixture, &big, "other", &format!("warm{count}"));
        merges.push((
            count,
            sdk_merge(&fixture, &big, "other", &format!("m{count}")),
        ));
    }
    // The first publish moves main; each big Branch then skips the Layers
    // published since its base, through the same path.
    sdk_publish(&fixture, "other");
    for count in COUNTS {
        // Every big head has root `a`; a filler publication of `b` between
        // them keeps each one a change.
        let filler = format!("filler{count}");
        fixture.fork(&filler);
        commit(&fixture, &filler, b);
        sdk_publish(&fixture, &filler);
        let (work, published) = sdk_publish(&fixture, &format!("big{count}"));
        println!(
            "SCALE_353a commits_since_layer={count} publish={work:?} overwroteCount={:?}",
            published.get("overwroteCount")
        );
        publishes.push((count, work));
    }
    println!(
        "SCALE_353a ancestry={walks:?} merge={merges:?} elapsed_ms={} profile=Disposable",
        start.elapsed().as_millis()
    );
    flat("ancestry", &walks);
    flat("merge", &merges);
    flat("publish", &publishes);
    fixture.cleanup();
}

#[test]
fn b_layers_between_two_sides() {
    let fixture = fixture("scale-353b");
    let start = Instant::now();
    let [a, b, c] = <[ObjectId; 3]>::try_from(roots(&fixture, &["a\n", "b\n", "c\n"])).unwrap();
    fixture.fork("x");
    fixture.apply("x", vec![write("/lane/x.txt", "x\n")]);
    let genesis = fixture
        .service
        .store
        .history()
        .branch(id("x"))
        .unwrap()
        .unwrap()
        .base_layer;
    let (mut walks, mut merges, mut publishes) = (Vec::new(), Vec::new(), Vec::new());
    let mut layers = 0;
    for count in COUNTS {
        while layers < count {
            let name = format!("p{layers}");
            let main = fixture
                .service
                .store
                .history()
                .layer_stack(stack())
                .unwrap()
                .unwrap()
                .head_layer;
            fork(&fixture, &name, ForkSource::Layer(main));
            let root = if root_of(&fixture, NodeId::Layer(main)) == a {
                b
            } else {
                a
            };
            commit(&fixture, &name, root);
            publish(&fixture, &name);
            layers += 1;
        }
        let main = fixture
            .service
            .store
            .history()
            .layer_stack(stack())
            .unwrap()
            .unwrap()
            .head_layer;
        let y = format!("y{count}");
        fork(&fixture, &y, ForkSource::Layer(main));
        commit(&fixture, &y, c);
        let (work, base) = ancestry(&fixture, "x", &y);
        assert_eq!(base, Some(NodeId::Layer(genesis)));
        walks.push((count, work));
        sdk_merge(&fixture, "x", &y, &format!("warm{count}"));
        merges.push((count, sdk_merge(&fixture, "x", &y, &format!("m{count}"))));
        // A Branch from genesis publishes over every Layer since.
        let probe = format!("probe{count}");
        fork(&fixture, &probe, ForkSource::Layer(genesis));
        commit(&fixture, &probe, c);
        let before = fixture.work();
        let (layer, overwrote) = publish(&fixture, &probe);
        publishes.push((count, fixture.since(before)));
        assert_eq!(overwrote as usize, layers);
        // The whole list pages out: the newest is the Layer below the probe's,
        // the oldest is the one just above genesis.
        let history = fixture.service.store.history();
        let parent = history.layer(layer).unwrap().unwrap().parent.unwrap();
        assert_eq!(history.overwritten(layer, 0, 128).unwrap()[0], parent);
        let last = history.overwritten(layer, overwrote - 1, 128).unwrap();
        assert_eq!(last.len(), 1);
        assert_eq!(
            history.layer(last[0]).unwrap().unwrap().parent,
            Some(genesis)
        );
        assert!(history
            .overwritten(layer, overwrote, 128)
            .unwrap()
            .is_empty());
        layers += 1;
        println!(
            "SCALE_353b layers_between={count} elapsed_ms={} overwrote={overwrote}",
            start.elapsed().as_millis()
        );
    }
    let (work, published) = sdk_publish(&fixture, "x");
    let id = published
        .get("publishedLayer")
        .and_then(|l| l.get_text("id"))
        .unwrap()
        .to_owned();
    let total = published
        .get("overwroteCount")
        .and_then(|c| match c {
            SdkValue::Int(n) => Some(*n),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        published.get("overwroteHasMore"),
        Some(&SdkValue::Bool(true))
    );
    // The full list is the backend's (owner 2026-10-19); the public table
    // refuses the old name.
    let fields = vec![
        ("layer", text(&id)),
        ("pageSize", SdkValue::Int(1000)),
        ("pagePos", SdkValue::Int(total - 10)),
    ];
    let refusal = fixture
        .try_call("project.list_overwritten", fields.clone())
        .unwrap_err();
    assert_eq!(refusal.code, "UnknownMethod");
    let call = SdkCall {
        method: "backend.store.project.list_overwritten".into(),
        args: SdkValue::Map(
            fields
                .into_iter()
                .chain([("project", text("installed"))])
                .map(|(name, value)| (name.to_owned(), value))
                .collect(),
        ),
    };
    let tail = operator::dispatch(&fixture.service, &call).unwrap();
    assert_eq!(support::items(&tail).len(), 10, "{tail:?}");
    assert_eq!(tail.get("hasMore"), Some(&SdkValue::Bool(false)));
    println!("SCALE_353b ancestry={walks:?} merge={merges:?} publish_probe={publishes:?} publish_x={work:?} overwroteCount={:?} profile=Disposable", published.get("overwroteCount"));
    flat("ancestry", &walks);
    flat("merge", &merges);
    flat("publish", &publishes);
    fixture.cleanup();
}

#[test]
fn c_fork_from_a_deep_commit() {
    let fixture = fixture("scale-353c");
    let [a, b] = <[ObjectId; 2]>::try_from(roots(&fixture, &["a\n", "b\n"])).unwrap();
    fixture.fork("deep");
    fixture.apply("deep", vec![write("/lane/deep.txt", "deep 0\n")]);
    let first = hex(head(&fixture, "deep"));
    let mut done = 0;
    let mut forks = Vec::new();
    for count in COUNTS {
        let mut current = root_of(&fixture, head(&fixture, "deep"));
        for _ in done..count {
            current = if current == a { b } else { a };
            commit(&fixture, "deep", current);
        }
        done = count;
        let near = hex(head(&fixture, "deep"));
        fixture.call(
            "branch.fork",
            vec![
                ("name", text(&format!("near{count}"))),
                ("from", text(&near)),
            ],
        );
        let before = fixture.work();
        fixture.call(
            "branch.fork",
            vec![
                ("name", text(&format!("old{count}"))),
                ("from", text(&first)),
            ],
        );
        forks.push((count, fixture.since(before)));
    }
    println!("SCALE_353c first_parent_depth={COUNTS:?} fork={forks:?} profile=Disposable");
    flat("fork", &forks);
    fixture.cleanup();
}

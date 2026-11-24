//! The bounded retained-`done` audit read: oldest first, page/limit, and
//! invisible to admission. The live queries keep filtering `state <> 'done'`.
mod support;

use layerfs_history::records::{GcIntent, GcIntentKind, GcScopeKind};
use layerfs_history::{
    BranchId, HistoryCatalog, HistoryCatalogConfig, LayerStackId, PlaceAuthority,
};
use layerfs_persistence::HistoryProvider;
use rusqlite::params;
use support::{branch_id, name, root, stack, Temp};

const BINDING: &[u8] = b"gc-done-audit";
const CURSOR: [u8; 32] = [65; 32];

fn catalog(temp: &Temp) -> HistoryProvider {
    support::sqlite::create(
        &temp.join("store.sqlite"),
        &HistoryCatalogConfig {
            cursor_key: CURSOR,
            binding_key: BINDING.to_vec(),
            incarnation: 7,
        },
    )
    .expect("catalog")
}

fn project(history: &dyn HistoryCatalog) -> (LayerStackId, BranchId) {
    history
        .initialize_layerstack(&layerfs_history::records::StackInitialization {
            logical: 1,
            stack: stack(1),
            name: name("proj"),
            scope: root(30),
            profile: root(31),
            genesis_root: root(1),
        })
        .unwrap();
    let branch = branch_id(1);
    history
        .fork(&layerfs_history::records::ForkRequest {
            stack: stack(1),
            branch,
            name: name("work"),
            source: layerfs_history::records::ForkSource::Layer(
                history.layer_stack(stack(1)).unwrap().unwrap().head_layer,
            ),
        })
        .unwrap();
    (stack(1), branch)
}

fn op(body: u8) -> [u8; 16] {
    [body; 16]
}

/// Inserts one retained done row, as an operator release leaves it: owner
/// columns cleared so the unique owner key can never block a remount.
fn done_row(path: &std::path::Path, branch: BranchId, body: u8, evidence: &str, updated_ms: i64) {
    let connection = rusqlite::Connection::open(path).unwrap();
    connection
        .execute(
            "INSERT INTO gc_intent (
                op_id, kind, scope_kind, scope_id, workspace, sandbox, machine,
                project, branch, owner_runtime, owner_boot, state, uncertain,
                phase, evidence, updated_ms
             ) VALUES (?1, 'mount', 'branch', ?2, NULL, 'box', 'box',
                'proj', 'work', NULL, NULL, 'done', 0, 'release', ?3, ?4)",
            params![op(body).as_slice(), branch.as_slice(), evidence, updated_ms],
        )
        .unwrap();
}

#[test]
fn the_done_audit_reads_oldest_first_in_bounded_pages() {
    let temp = Temp::new("gc-done-audit-page");
    let history = catalog(&temp);
    let (_, branch) = project(&history);
    let path = temp.join("store.sqlite");
    done_row(&path, branch, 3, "release three", 30);
    done_row(&path, branch, 1, "release one", 10);
    done_row(&path, branch, 2, "release two", 20);

    let first = history.gc_done_intents(0, 2).unwrap();
    assert_eq!(first.page, 0);
    assert_eq!(first.limit, 2);
    assert!(first.has_more);
    assert_eq!(first.items.len(), 2);
    assert_eq!(first.items[0].op_id, op(1));
    assert_eq!(first.items[0].evidence.as_deref(), Some("release one"));
    assert_eq!(first.items[1].op_id, op(2));

    let second = history.gc_done_intents(1, 2).unwrap();
    assert!(!second.has_more);
    assert_eq!(second.items.len(), 1);
    assert_eq!(second.items[0].op_id, op(3));
    assert_eq!(second.items[0].updated_ms, 30);

    // The limit is bounded, not the caller's number.
    let clamped = history.gc_done_intents(0, u32::MAX).unwrap();
    assert_eq!(clamped.limit, layerfs_history::MAXIMUM_AUDIT_PAGE);
}

/// The operator release is the writer of the retained row: the owning mount
/// keeps its op, its place and its phase, loses its owner columns, and gains
/// the action/operator/op evidence. A second release finds no live row and
/// invents no second audit row.
#[test]
fn release_retains_the_operator_audit_row_and_clears_the_owner() {
    let temp = Temp::new("gc-done-audit-release");
    let history = catalog(&temp);
    let (_, branch) = project(&history);
    let mounted = GcIntent::shared(
        op(20),
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch.as_slice(),
    )
    .in_project(stack(1).as_slice())
    .named(Some("proj"), Some("work"))
    .placed(Some([20; 32]), Some("box"), Some("box"))
    .owned(Some("pid:1@0"), Some("boot:1"))
    .with_phase(Some("bind"))
    .authorized(PlaceAuthority {
        machine_incarnation: Some("worker:1".to_owned()),
        sandbox_generation: Some("generation:1".to_owned()),
    });
    history.gc_register(&mounted).unwrap();
    history.gc_mark_uncertain(&op(20)).unwrap();

    let evidence = "action=release;operator=alice;op=14141414141414141414141414141414";
    assert!(history.gc_release_done(&op(20), evidence).unwrap());
    // The row is gone from the live read and from a second release.
    assert!(history
        .gc_intents(GcScopeKind::Branch, branch.as_slice())
        .unwrap()
        .is_empty());
    assert!(!history
        .gc_release_done(&op(20), evidence)
        .expect("an absent row is not an error"));

    let page = history.gc_done_intents(0, 10).unwrap();
    assert_eq!(page.items.len(), 1, "{page:?}");
    let row = &page.items[0];
    assert_eq!(row.op_id, op(20));
    assert_eq!(row.kind, GcIntentKind::Mount);
    assert_eq!(row.scope_kind, GcScopeKind::Branch);
    assert_eq!(row.scope_id.as_slice(), branch.as_slice());
    assert_eq!(row.project.as_deref(), Some("proj"));
    assert_eq!(row.branch.as_deref(), Some("work"));
    assert_eq!(row.machine.as_deref(), Some("box"));
    assert_eq!(row.sandbox.as_deref(), Some("box"));
    assert_eq!(row.phase.as_deref(), Some("bind"));
    assert_eq!(row.evidence.as_deref(), Some(evidence));
    assert!(row.updated_ms > 0);

    // The cleared owner key no longer blocks the same owner and scope.
    let remounted = GcIntent::shared(
        op(21),
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch.as_slice(),
    )
    .in_project(stack(1).as_slice())
    .named(Some("proj"), Some("work"))
    .placed(Some([21; 32]), Some("box"), Some("box"))
    .owned(Some("pid:1@0"), Some("boot:1"));
    history
        .gc_register(&remounted)
        .expect("the retained row's cleared owner key does not hold");
}

#[test]
fn a_done_row_is_never_a_live_holder() {
    let temp = Temp::new("gc-done-audit-hidden");
    let history = catalog(&temp);
    let (_, branch) = project(&history);
    let path = temp.join("store.sqlite");
    done_row(&path, branch, 9, "release nine", 10);

    // The live read on the scope does not report it.
    assert!(history
        .gc_intents(GcScopeKind::Branch, branch.as_slice())
        .unwrap()
        .is_empty());

    // A later mount on the Branch is admitted: the done row's cleared owner
    // key cannot block it, and admission filters done.
    let mount = GcIntent::shared(
        op(10),
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch.as_slice(),
    )
    .in_project(stack(1).as_slice())
    .named(Some("proj"), Some("work"))
    .placed(Some([10; 32]), Some("box"), Some("box"))
    .owned(Some("runtime-private"), Some("boot-private"))
    .authorized(PlaceAuthority::default());
    history
        .gc_register(&mount)
        .expect("the done row does not hold");
}

/// The operator audit is scoped to the Branch it names: a done row of one
/// Branch never appears in another Branch's page, even at the same Store.
#[test]
fn a_branch_audit_reads_only_its_own_done_rows() {
    let temp = Temp::new("gc-done-audit-scope");
    let path = temp.join("store.sqlite");
    let history = catalog(&temp);
    let (_, first) = project(&history);
    let second = branch_id(2);
    history
        .fork(&layerfs_history::records::ForkRequest {
            stack: stack(1),
            branch: second,
            name: name("other"),
            source: layerfs_history::records::ForkSource::Layer(
                history.layer_stack(stack(1)).unwrap().unwrap().head_layer,
            ),
        })
        .unwrap();
    done_row(&path, first, 0x01, "action=preview;operator=alice", 10);
    done_row(&path, second, 0x02, "action=preview;operator=alice", 20);
    // The done row keeps the Branch name it recorded, so the scoped audit
    // reads it even after the Branch itself is removed.
    {
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute(
                "UPDATE gc_intent SET branch = 'other' WHERE op_id = ?1",
                [op(0x02).as_slice()],
            )
            .unwrap();
    }

    let page = history
        .gc_done_intents_branch("proj", "work", None, 10)
        .unwrap();
    assert_eq!(page.items.len(), 1, "only the named Branch's row");
    assert_eq!(page.items[0].op_id, op(0x01));
    let other = history
        .gc_done_intents_branch("proj", "other", None, 10)
        .unwrap();
    assert_eq!(other.items.len(), 1);
    assert_eq!(other.items[0].op_id, op(0x02));
}

fn stored(path: &std::path::Path, body: u8) -> (String, Option<String>) {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row(
            "SELECT state, evidence FROM gc_intent WHERE op_id = ?1",
            params![op(body).as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
}

/// The place tokens live in the base format's `evidence` column: a live row
/// round-trips both, a value without the `place:v1` prefix (a `done` row's
/// audit text) or a malformed one decodes as no tokens, and the live-path
/// release, uncertain and insert-conflict statements neither delete nor
/// revive a retained `done` row.
#[test]
fn place_tokens_round_trip_through_evidence_and_live_statements_keep_a_done_row() {
    let temp = Temp::new("gc-done-place-evidence");
    let path = temp.join("store.sqlite");
    let history = catalog(&temp);
    let (_, branch) = project(&history);
    let incarnation = "p1:worker:0000000000000010:aa";
    let generation = "p1:sandbox:0000000000000020:bb";
    let mounted = GcIntent::shared(
        op(40),
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch.as_slice(),
    )
    .in_project(stack(1).as_slice())
    .named(Some("proj"), Some("work"))
    .placed(Some([40; 32]), Some("box"), Some("box"))
    .owned(Some("pid:1@0"), Some("boot:40"))
    .authorized(PlaceAuthority {
        machine_incarnation: Some(incarnation.to_owned()),
        sandbox_generation: Some(generation.to_owned()),
    });
    history.gc_register(&mounted).unwrap();
    let encoded = format!("place:v1;inc={incarnation};gen={generation}");
    assert_eq!(stored(&path, 40), ("live".to_owned(), Some(encoded)));
    let tokens = |history: &HistoryProvider| {
        let listed = history
            .gc_intents(GcScopeKind::Branch, branch.as_slice())
            .unwrap();
        assert_eq!(listed.len(), 1, "{listed:?}");
        (
            listed[0].machine_incarnation.clone(),
            listed[0].sandbox_generation.clone(),
        )
    };
    assert_eq!(
        tokens(&history),
        (Some(incarnation.to_owned()), Some(generation.to_owned()))
    );

    // Foreign or malformed evidence is no proof: no token at all.
    let connection = rusqlite::Connection::open(&path).unwrap();
    for text in [
        "place:v2;inc=a",
        "place:v1",
        "place:v1;inc=",
        "place:v1;inc=a;inc=b",
        "place:v1;who=a",
    ] {
        connection
            .execute(
                "UPDATE gc_intent SET evidence = ?1 WHERE op_id = ?2",
                params![text, op(40).as_slice()],
            )
            .unwrap();
        assert_eq!(tokens(&history), (None, None), "{text}");
    }
    connection
        .execute(
            "UPDATE gc_intent SET evidence = ?1 WHERE op_id = ?2",
            params![format!("place:v1;gen={generation}"), op(40).as_slice()],
        )
        .unwrap();
    assert_eq!(tokens(&history), (None, Some(generation.to_owned())));

    // The operator retains the row as `done`; its evidence is the audit text.
    let audit = "action=release;operator=alice;op=28282828282828282828282828282828";
    assert!(history.gc_release_done(&op(40), audit).unwrap());
    let done = ("done".to_owned(), Some(audit.to_owned()));
    assert_eq!(stored(&path, 40), done);

    // The owner's own release, a late uncertain mark and a retried
    // registration under the same op id all leave the done row as it was.
    history.gc_release(&op(40)).unwrap();
    assert_eq!(stored(&path, 40), done, "release keeps the done row");
    history.gc_mark_uncertain(&op(40)).unwrap();
    assert_eq!(stored(&path, 40), done, "uncertain keeps the done row");
    assert!(matches!(
        history.gc_register(&mounted),
        Err(layerfs_history::HistoryError::Conflict(_))
    ));
    assert_eq!(
        stored(&path, 40),
        done,
        "a retry does not revive the done row"
    );
    assert!(history
        .gc_intents(GcScopeKind::Branch, branch.as_slice())
        .unwrap()
        .is_empty());

    // Read as a live row, the done row's audit text decodes as no tokens.
    connection
        .execute(
            "UPDATE gc_intent SET state = 'live' WHERE op_id = ?1",
            params![op(40).as_slice()],
        )
        .unwrap();
    assert_eq!(tokens(&history), (None, None));
}

/// #783 gap 3: one Branch's retained `done` audit is bounded with no schema
/// change. A release that retains a row deletes that Branch's oldest done
/// rows beyond the newest 100 (`DONE_RETAINED`); another Branch's rows and
/// the Branch's own live and uncertain intents are untouched.
#[test]
fn a_release_keeps_at_most_the_newest_done_rows_of_its_branch() {
    const RETAINED: usize = 100;
    let temp = Temp::new("gc-done-audit-cap");
    let history = catalog(&temp);
    let (_, branch) = project(&history);
    let path = temp.join("store.sqlite");
    for body in 1..=RETAINED as u8 {
        done_row(&path, branch, body, "release", i64::from(body));
    }
    // another Branch's done row is not this Branch's to bound
    let other = branch_id(2);
    history
        .fork(&layerfs_history::records::ForkRequest {
            stack: stack(1),
            branch: other,
            name: name("other"),
            source: layerfs_history::records::ForkSource::Layer(
                history.layer_stack(stack(1)).unwrap().unwrap().head_layer,
            ),
        })
        .unwrap();
    done_row(&path, other, 250, "release other", 0);
    let mounted = GcIntent::shared(
        op(200),
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch.as_slice(),
    )
    .in_project(stack(1).as_slice())
    .named(Some("proj"), Some("work"))
    .placed(Some([200; 32]), Some("box"), Some("box"))
    .owned(Some("pid:1@0"), Some("boot:1"));
    history.gc_register(&mounted).unwrap();
    assert!(history.gc_mark_uncertain(&op(200)).unwrap());
    // a live and an uncertain holder of the same Branch, which the cap's
    // `state = 'done'` never matches
    history.gc_register(&holder(branch, 201)).unwrap();
    history.gc_register(&holder(branch, 202)).unwrap();
    assert!(history.gc_mark_uncertain(&op(202)).unwrap());
    assert!(history.gc_release_done(&op(200), "release").unwrap());

    // the test rows all record the names proj/work: count by Branch scope
    let connection = rusqlite::Connection::open(&path).unwrap();
    let kept = |scope: BranchId| -> Vec<Vec<u8>> {
        let mut statement = connection
            .prepare("SELECT op_id FROM gc_intent WHERE state = 'done' AND scope_id = ?1")
            .unwrap();
        let rows = statement
            .query_map([scope.as_slice()], |row| row.get(0))
            .unwrap();
        rows.map(Result::unwrap).collect()
    };
    let work = kept(branch);
    assert_eq!(work.len(), RETAINED, "the Branch keeps its newest rows");
    assert!(!work.contains(&op(1).to_vec()), "the oldest went");
    assert!(work.contains(&op(2).to_vec()) && work.contains(&op(200).to_vec()));
    assert_eq!(kept(other).len(), 1, "another Branch's row stays");
    assert_eq!(stored(&path, 201).0, "live", "the live holder stays");
    assert_eq!(stored(&path, 202).0, "uncertain", "the uncertain one too");
    let live = history
        .gc_intents(GcScopeKind::Branch, branch.as_slice())
        .unwrap();
    assert_eq!(live.len(), 2, "{live:?}");
}

/// A live mount of `proj/work` with its own owner boot, so several can be
/// held at one Branch at once.
fn holder(branch: BranchId, body: u8) -> GcIntent {
    GcIntent::shared(
        op(body),
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch.as_slice(),
    )
    .in_project(stack(1).as_slice())
    .named(Some("proj"), Some("work"))
    .placed(Some([body; 32]), Some("box"), Some("box"))
    .owned(Some("pid:1@0"), Some(&format!("boot:{body}")))
}

/// Registers, settles and releases one holder, retaining its `done` row.
fn release_one(history: &HistoryProvider, branch: BranchId, body: u8) {
    history.gc_register(&holder(branch, body)).unwrap();
    assert!(history.gc_mark_uncertain(&op(body)).unwrap());
    assert!(history.gc_release_done(&op(body), "release").unwrap());
}

/// Every audit page of `proj/work` of `limit` rows, read from `after`.
fn audit_page(
    history: &HistoryProvider,
    after: Option<layerfs_history::GcAuditAfter>,
    limit: u32,
) -> layerfs_history::GcBranchAuditPage {
    history
        .gc_done_intents_branch("proj", "work", after, limit)
        .unwrap()
}

/// #784 review: the audit pages by `(updated_ms, op_id)`, not by offset. A
/// release between two page reads deletes the Branch's oldest row (the cap)
/// and retains a newer one; the next page still starts right after the last
/// row read, so no row is skipped or repeated, and the new rows come last.
#[test]
fn a_release_between_audit_pages_neither_skips_nor_repeats_a_row() {
    let temp = Temp::new("gc-done-audit-keyset");
    let history = catalog(&temp);
    let (_, branch) = project(&history);
    let path = temp.join("store.sqlite");
    // two rows share a time: the op orders them
    for body in 1..=100u8 {
        let ms = i64::from(body.min(99));
        done_row(&path, branch, body, "release", ms);
    }
    let mut seen: Vec<[u8; 16]> = Vec::new();
    let mut after = None;
    let mut pages = 0;
    loop {
        let page = audit_page(&history, after, 10);
        assert_eq!(page.limit, 10);
        seen.extend(page.items.iter().map(|row| row.op_id));
        pages += 1;
        // the first two reads are each followed by a release: the cap
        // deletes the oldest row (already read) and the new row comes last
        if pages <= 2 {
            release_one(&history, branch, 200 + pages);
        }
        match page.next {
            Some(next) => {
                let last = page.items.last().unwrap();
                assert_eq!((next.updated_ms, next.op_id), (last.updated_ms, last.op_id));
                after = Some(next);
            }
            None => break,
        }
        assert!(pages < 20, "the pages end");
    }
    let mut expected: Vec<[u8; 16]> = (1..=100u8).map(op).collect();
    expected.extend([op(201), op(202)]);
    assert_eq!(seen, expected, "every row once, oldest first");
    let clamped = audit_page(&history, None, u32::MAX);
    assert_eq!(clamped.limit, layerfs_history::MAXIMUM_AUDIT_PAGE);
    assert!(clamped.next.is_none());
}

/// #784 review: the cap orders by the releasing host's clock only through
/// the stamp. A release on a host whose clock is behind the Branch's newest
/// done row (here a full audit stamped an hour ahead) is stamped after that
/// row: the cap in the same transaction keeps it, and the audit returns it
/// last.
#[test]
fn a_release_stamped_behind_the_newest_row_is_kept_and_listed_last() {
    let temp = Temp::new("gc-done-audit-clock");
    let history = catalog(&temp);
    let (_, branch) = project(&history);
    let path = temp.join("store.sqlite");
    let ahead = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
        + 3_600_000;
    for body in 1..=100u8 {
        done_row(&path, branch, body, "release", ahead + i64::from(body));
    }
    release_one(&history, branch, 200);

    let page = audit_page(&history, None, 1000);
    assert_eq!(page.items.len(), 100, "the cap keeps the newest 100");
    let last = page.items.last().unwrap();
    assert_eq!(last.op_id, op(200), "the new release is retained and last");
    assert_eq!(last.updated_ms, ahead + 101, "stamped after the newest row");
    assert!(
        page.items.iter().all(|row| row.op_id != op(1)),
        "the oldest row went instead"
    );
}

//! Complete-operation runtime database profiles for S4, paired with the
//! engine's statement plans. Deterministic work diagnostics, not timings.
mod common;
mod harness;
use harness::*;
use layerfs_workspace::Operation;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Work {
    rounds: u64,
    statements: u64,
    vm_steps: u64,
    rows_changed: u64,
    base_demand: u64,
}
/// Everything one complete operation costs the owner: its source window, all
/// owner rounds, the single publication and its reply-attempt release.
fn measure(b: &Bench, work: impl FnOnce()) -> Work {
    let (before, rounds, demand) = (b.overlay.diagnostics(), b.rounds.get(), b.demand());
    work();
    let after = b.overlay.diagnostics();
    let mut total = Work {
        rounds: b.rounds.get() - rounds,
        statements: 0,
        vm_steps: 0,
        rows_changed: 0,
        base_demand: b.demand() - demand,
    };
    for (index, (a, z)) in before.statements.iter().zip(&after.statements).enumerate() {
        assert_eq!(
            z.fullscan_steps, a.fullscan_steps,
            "fullscan in family {index}"
        );
        assert_eq!(z.sorts, a.sorts, "sort in family {index}");
        assert_eq!(
            z.autoindex_rows, a.autoindex_rows,
            "autoindex in family {index}"
        );
        assert_eq!(z.reprepares, a.reprepares, "reprepare in family {index}");
        total.statements += z.executions - a.executions;
        total.vm_steps += z.vm_steps - a.vm_steps;
        total.rows_changed += z.rows_changed - a.rows_changed;
    }
    total
}

#[test]
fn complete_operations_keep_point_work_as_names_and_inodes_grow() {
    // No object cache, so base demand per operation is the same at every scale.
    let b = Bench::with_cache("profile", 0);
    let local = b.applied(mkdir(1, "local"), T1).unwrap().serial;
    let nested = b.applied(mkdir(local, "nested"), T1).unwrap().serial;
    // The linked inherited file already has a local row at every scale, so
    // the comparison isolates growth from the first-touch fact round.
    b.applied(link(8, local, "first-link"), T1);
    let source = b
        .overlay
        .acquire_base_source(b.route(), u64::MAX >> 1)
        .unwrap();
    let plans = b.overlay.explain_compound(source).unwrap();
    b.overlay.release_base_source(source).unwrap();
    for plan in &plans {
        assert!(!plan.contains("SCAN") && !plan.contains("TEMP"), "{plan}");
    }
    println!("S4_PLANS {plans:?}");

    let mut filled = 0;
    let mut profiles: Vec<Vec<(&str, Work)>> = Vec::new();
    for scale in [128, 1024, 4096] {
        // Siblings in the inherited root and in the local directory operated on.
        while filled < scale {
            b.applied(create(1, &format!("sibling-{filled:05}")), T1);
            b.applied(create(local, &format!("sibling-{filled:05}")), T1);
            filled += 1;
        }
        let tag = |what: &str| format!("op-{scale}-{what}");
        let mut row = Vec::new();
        row.push((
            "create-inherited-parent",
            measure(&b, || {
                b.applied(create(1, &tag("a")), T1);
            }),
        ));
        row.push((
            "create-local-parent",
            measure(&b, || {
                b.applied(create(local, &tag("a")), T1);
            }),
        ));
        row.push((
            "link-file",
            measure(&b, || {
                b.applied(link(8, local, &tag("l")), T1);
            }),
        ));
        b.applied(create(1, &tag("b")), T1);
        row.push((
            "rename-replace",
            measure(&b, || {
                b.applied(rename((1, &tag("a")), (1, &tag("b")), true, None), T2);
            }),
        ));
        row.push((
            "unlink",
            measure(&b, || {
                b.applied(unlink(1, &tag("b")), T2);
            }),
        ));
        row.push((
            "mkdir",
            measure(&b, || {
                b.applied(mkdir(1, &tag("d")), T1);
            }),
        ));
        row.push((
            "rename-directory-across-parents",
            measure(&b, || {
                b.applied(
                    rename(
                        (1, &tag("d")),
                        (nested, &tag("d")),
                        true,
                        path(&["local", "nested"]),
                    ),
                    T2,
                );
            }),
        ));
        row.push((
            "rmdir",
            measure(&b, || {
                b.applied(rmdir(nested, &tag("d")), T2);
            }),
        ));
        row.push((
            "chmod-utimens",
            measure(&b, || {
                b.applied(
                    Operation::SetAttributes {
                        serial: local,
                        mode: Some(0o700 + (scale as u32 % 7)),
                        mtime: Some(T2),
                        size: None,
                    },
                    T1,
                );
            }),
        ));
        row.push((
            "lookup-local",
            measure(&b, || {
                assert!(b.lookup(local, &tag("a")).is_some());
            }),
        ));
        row.push((
            "lookup-inherited",
            measure(&b, || {
                assert!(b.lookup(1, "file").is_some());
            }),
        ));
        row.push((
            "list-window",
            measure(&b, || {
                let page = b
                    .window(|view| view.list(&b.overlay, local, Some(b"sibling-00010")))
                    .unwrap();
                assert_eq!((page.entries.len(), page.visited), (64, 64));
            }),
        ));
        for (operation, work) in &row {
            println!(
                "S4_OPERATION siblings={scale} operation={operation} owner_rounds={} statements={} vm_steps={} rows_changed={} base_demand={} fullscan=0 sorts=0 autoindex=0 reprepare=0",
                work.rounds, work.statements, work.vm_steps, work.rows_changed, work.base_demand
            );
        }
        profiles.push(row);
    }
    for row in &profiles[1..] {
        assert_eq!(row, &profiles[0], "work must not grow with unrelated rows");
    }
    // A local parent settles in one owner round with no base demand; an
    // inherited parent needs one fact round for the name's base absence.
    let work = |name: &str| profiles[0].iter().find(|(n, _)| *n == name).unwrap().1;
    assert_eq!(
        (
            work("create-local-parent").rounds,
            work("create-local-parent").base_demand
        ),
        (1, 0)
    );
    assert_eq!(work("create-inherited-parent").rounds, 2);
    // The stored active inheritance fact decides this local source name,
    // avoiding a base-name round while retaining exact facts.
    assert_eq!(work("rename-directory-across-parents").rounds, 1);
    assert_eq!(work("rmdir").rounds, 1);
    assert_eq!(work("lookup-local").base_demand, 0);
}

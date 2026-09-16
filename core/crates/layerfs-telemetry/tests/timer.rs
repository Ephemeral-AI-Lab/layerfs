//! Public-API behavior of the timer: nesting, results, bounds and attachment.

use std::time::Duration;

use layerfs_telemetry::timer::{
    Active, NodeOutcome, Timing, TimingNode, TimingReport, TimingScope, MAX_DEPTH, MAX_LABEL_BYTES,
    MAX_NODES,
};

#[derive(Debug, PartialEq, Eq)]
enum TestError {
    Boom,
    Transient,
}

fn ok<T>(value: T) -> Result<T, TestError> {
    Ok(value)
}

fn child_names(node: &TimingNode) -> Vec<&str> {
    node.children().iter().map(TimingNode::name).collect()
}

fn assert_inclusive(node: &TimingNode) {
    for child in node.children() {
        assert!(
            node.elapsed() >= child.elapsed(),
            "{} ({:?}) must include {} ({:?})",
            node.name(),
            node.elapsed(),
            child.name(),
            child.elapsed()
        );
        assert_inclusive(child);
    }
}

fn assert_ascending_elapsed(node: &TimingNode) {
    for child in node.children() {
        assert!(
            node.elapsed() >= child.elapsed(),
            "child elapsed exceeds parent elapsed"
        );
        assert_ascending_elapsed(child);
    }
}

fn construct(scope: TimingScope<'_>) -> Result<u32, TestError> {
    scope.run(|content| {
        let bytes = content.child("encode").run(|_| ok(3_u32))?;
        let id = content.child("hash").run(|_| ok(bytes + 1))?;
        Ok(id)
    })
}

fn deepest(node: &TimingNode) -> &TimingNode {
    match node.children().first() {
        Some(child) => deepest(child),
        None => node,
    }
}

#[test]
fn nested_children_are_recorded_in_start_order() {
    let (result, report) = Timing::record("object.create", |root| {
        let id = construct(root.child("canonical.construct"))?;
        root.child("storage.save").run(|_| ok(id))
    });

    assert_eq!(result, Ok(4));
    let root = report.root().expect("a measured report has a root");
    assert_eq!(root.name(), "object.create");
    assert_eq!(root.outcome(), NodeOutcome::Ok);
    assert!(!root.is_incomplete());
    assert_eq!(child_names(root), ["canonical.construct", "storage.save"]);
    assert_eq!(child_names(&root.children()[0]), ["encode", "hash"]);
    assert_eq!(report.node_count(), 5);
    assert_eq!(report.levels(), 3);
    assert!(!report.is_incomplete());
}

#[test]
fn repeated_labels_are_distinct_invocations_in_start_order() {
    let (result, report) = Timing::record("root", |root| {
        for _ in 0..3 {
            root.child("chunk").run(|_| ok(()))?;
        }
        ok(())
    });

    assert_eq!(result, Ok(()));
    let root = report.root().expect("a measured report has a root");
    assert_eq!(child_names(root), ["chunk", "chunk", "chunk"]);
    assert_eq!(report.node_count(), 4);
}

#[test]
fn uncalled_operations_have_no_node() {
    let (result, report) = Timing::record("root", |root| {
        root.child("called").run(|_| ok(()))?;
        let _never = root.child("never");
        ok(())
    });

    assert_eq!(result, Ok(()));
    let root = report.root().expect("a measured report has a root");
    assert_eq!(child_names(root), ["called"]);
    assert_eq!(report.node_count(), 2);
}

#[test]
fn errors_are_recorded_before_early_returns_keep_completed_nodes() {
    let (result, report) = Timing::record("root", |root| {
        root.child("first").run(|_| ok(()))?;
        root.child("second").run(|_| Err(TestError::Boom))?;
        root.child("third").run(|_| ok(()))
    });

    assert_eq!(result, Err(TestError::Boom));
    let root = report.root().expect("a measured report has a root");
    assert_eq!(child_names(root), ["first", "second"]);
    assert_eq!(root.outcome(), NodeOutcome::Error);
    assert_eq!(root.children()[0].outcome(), NodeOutcome::Ok);
    assert_eq!(root.children()[1].outcome(), NodeOutcome::Error);
    assert!(!root.is_incomplete());
}

#[test]
fn a_parent_can_recover_from_a_child_error() {
    let (result, report) = Timing::record("root", |root| {
        let _ = root
            .child("attempt")
            .run(|_| Err::<(), TestError>(TestError::Transient));
        ok(7_u32)
    });

    assert_eq!(result, Ok(7));
    let root = report.root().expect("a measured report has a root");
    assert_eq!(root.outcome(), NodeOutcome::Ok);
    assert_eq!(root.children()[0].outcome(), NodeOutcome::Error);
    assert!(!root.is_incomplete());
}

#[test]
fn durations_are_inclusive_monotonic_elapsed_time() {
    let (result, report) = Timing::record("root", |root| {
        root.child("outer").run(|outer| {
            outer
                .child("inner")
                .run(|inner| inner.child("deepest").run(|_| ok(())))
        })
    });

    assert_eq!(result, Ok(()));
    let root = report.root().expect("a measured report has a root");
    assert_inclusive(root);
    assert_ascending_elapsed(root);
    assert_eq!(report.levels(), 4);
}

#[test]
fn independent_recordings_compose_without_shared_state() {
    let (result, report) = Timing::record("outer", |root| {
        let inner = root.child("call.inner").run(|call| {
            let (inner_result, inner_report) = Timing::record("inner", |inner_root| {
                inner_root.child("inner.child").run(|_| ok(1_u32))
            });
            call.attach(Some(inner_report));
            inner_result
        })?;
        ok(inner + 1)
    });

    assert_eq!(result, Ok(2));
    let root = report.root().expect("a measured report has a root");
    assert_eq!(child_names(root), ["call.inner"]);
    let call = &root.children()[0];
    assert_eq!(child_names(call), ["inner"]);
    assert_eq!(child_names(&call.children()[0]), ["inner.child"]);
    assert_eq!(report.node_count(), 4);
    assert!(!root.is_incomplete());
}

#[test]
fn disabled_recording_runs_the_operation_and_records_nothing() {
    let sample = TimingReport::from_root(TimingNode::new("remote", Duration::from_millis(1)));
    let mut bodies = 0_u32;

    let (result, report) = Timing::disabled("object.create", |root| {
        assert!(!root.is_recording());
        let child = root.child("canonical.construct");
        assert!(!child.is_recording());
        bodies += 1;
        child.run(|inner| {
            assert!(!inner.is_recording());
            inner.attach(Some(sample));
            bodies += 1;
            ok(5_u32)
        })
    });

    assert_eq!(result, Ok(5));
    assert_eq!(bodies, 2);
    assert!(!report.has_root());
    assert_eq!(report.node_count(), 0);
    assert_eq!(report.levels(), 0);
    assert!(report.is_incomplete());
    assert!(report.root().is_none());
}

#[test]
fn panic_unwinds_without_a_fabricated_report() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = std::panic::catch_unwind(|| {
        let (_result, _report) = Timing::record("panicking", |root| -> Result<(), TestError> {
            root.child("boom")
                .run(|_| -> Result<(), TestError> { panic!("measured panic") })
        });
    });
    std::panic::set_hook(hook);

    let payload = outcome.expect_err("the panic must propagate to the caller");
    let message = payload.downcast_ref::<&str>().copied().unwrap_or_default();
    assert_eq!(message, "measured panic");
}

#[test]
fn a_panic_caught_inside_the_operation_leaves_the_recording_usable() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let (result, report) = Timing::record("root", |root| {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            root.child("unstable")
                .run(|_| -> Result<(), TestError> { panic!("child panic") })
        }));
        assert!(outcome.is_err());
        root.child("after").run(|_| ok(()))?;
        ok(9_u32)
    });
    std::panic::set_hook(hook);

    assert_eq!(result, Ok(9));
    let root = report.root().expect("a measured report has a root");
    assert_eq!(child_names(root), ["unstable", "after"]);
    let unstable = &root.children()[0];
    assert!(unstable.is_incomplete());
    assert_eq!(unstable.elapsed(), Duration::ZERO);
    assert_eq!(root.children()[1].outcome(), NodeOutcome::Ok);
    assert!(root.is_incomplete());
}

#[test]
fn long_labels_are_clipped_on_a_utf8_boundary() {
    let long = "x".repeat(MAX_LABEL_BYTES + 10);
    let (result, report) = Timing::record(long.clone(), |root| root.child(long).run(|_| ok(())));

    assert_eq!(result, Ok(()));
    let root = report.root().expect("a measured report has a root");
    assert_eq!(root.name().len(), MAX_LABEL_BYTES);
    assert!(root.is_incomplete());
    assert_eq!(root.children()[0].name().len(), MAX_LABEL_BYTES);
    assert!(root.children()[0].is_incomplete());

    let multibyte = "é".repeat(MAX_LABEL_BYTES);
    let (result, report) = Timing::record(multibyte, |root| root.child("leaf").run(|_| ok(())));
    assert_eq!(result, Ok(()));
    let root = report.root().expect("a measured report has a root");
    assert_eq!(root.name().len(), MAX_LABEL_BYTES);
    assert_eq!(root.name().chars().count(), MAX_LABEL_BYTES / 2);
    assert!(root.is_incomplete());
}

#[test]
fn node_budget_clips_detail_and_keeps_running_the_operation() {
    let mut bodies = 0_usize;
    let mut clipped = 0_usize;

    let (result, report) = Timing::record("root", |root| {
        for index in 0..MAX_NODES {
            let child = root.child(format!("child.{index}"));
            if !child.is_recording() {
                clipped += 1;
            }
            child.run(|_| {
                bodies += 1;
                ok(())
            })?;
        }
        ok(())
    });

    assert_eq!(result, Ok(()));
    assert_eq!(bodies, MAX_NODES);
    assert_eq!(clipped, 1);
    let root = report.root().expect("a measured report has a root");
    assert_eq!(report.node_count(), MAX_NODES);
    assert_eq!(root.children().len(), MAX_NODES - 1);
    assert!(root.is_incomplete());
    assert_eq!(
        root.children()[MAX_NODES - 2].name(),
        format!("child.{}", MAX_NODES - 2)
    );
}

fn descend(
    scope: &TimingScope<'_, Active>,
    remaining: usize,
    bodies: &mut usize,
) -> Result<(), TestError> {
    if remaining == 0 {
        return ok(());
    }
    scope.child("level").run(|child| {
        *bodies += 1;
        descend(child, remaining - 1, bodies)
    })
}

#[test]
fn depth_budget_clips_detail_and_keeps_running_the_operation() {
    let mut bodies = 0_usize;
    let (result, report) = Timing::record("root", |root| descend(root, 40, &mut bodies));

    assert_eq!(result, Ok(()));
    assert_eq!(bodies, 40);
    assert_eq!(report.levels(), usize::from(MAX_DEPTH));
    assert_eq!(report.node_count(), usize::from(MAX_DEPTH));
    let root = report.root().expect("a measured report has a root");
    assert_eq!(child_names(root), ["level"]);
    let leaf = deepest(root);
    assert!(leaf.is_incomplete());
    assert!(root.is_incomplete());
}

#[test]
fn attached_reports_become_owned_children() {
    let attached = TimingReport::from_root(
        TimingNode::new(String::from("remote.execute"), Duration::from_millis(5)).with_children(
            vec![TimingNode::new("remote.inner", Duration::from_micros(2))
                .with_outcome(NodeOutcome::Error)],
        ),
    );

    let (result, report) = Timing::record("caller", |root| {
        root.child("call").run(|active| {
            active.attach(Some(attached));
            ok(3_u32)
        })
    });

    assert_eq!(result, Ok(3));
    let root = report.root().expect("a measured report has a root");
    let call = &root.children()[0];
    assert_eq!(child_names(call), ["remote.execute"]);
    let grafted = &call.children()[0];
    assert_eq!(grafted.elapsed(), Duration::from_millis(5));
    assert_eq!(child_names(grafted), ["remote.inner"]);
    assert_eq!(grafted.children()[0].elapsed(), Duration::from_micros(2));
    assert_eq!(grafted.children()[0].outcome(), NodeOutcome::Error);
    assert!(!root.is_incomplete());
}

#[test]
fn missing_or_disabled_detail_marks_the_node_and_its_ancestors() {
    for attachment in [None, Some(TimingReport::disabled())] {
        let (result, report) = Timing::record("root", |root| {
            root.child("outer").run(|outer| {
                outer.child("call").run(|active| {
                    active.attach(attachment.clone());
                    ok(())
                })
            })
        });

        assert_eq!(result, Ok(()));
        let root = report.root().expect("a measured report has a root");
        assert!(root.is_incomplete());
        assert!(root.children()[0].is_incomplete());
        assert!(root.children()[0].children()[0].is_incomplete());
        assert_eq!(root.outcome(), NodeOutcome::Ok);
        assert_eq!(root.children()[0].children()[0].outcome(), NodeOutcome::Ok);
    }
}

#[test]
fn an_imported_incomplete_node_marks_the_attachment_point() {
    let attached = TimingReport::from_root(
        TimingNode::new("remote.execute", Duration::from_millis(1)).with_incomplete(true),
    );

    let (result, report) = Timing::record("caller", |root| {
        root.child("call").run(|active| {
            active.attach(Some(attached));
            ok(())
        })
    });

    assert_eq!(result, Ok(()));
    let root = report.root().expect("a measured report has a root");
    assert!(root.is_incomplete());
    assert!(root.children()[0].is_incomplete());
    assert!(root.children()[0].children()[0].is_incomplete());
}

#[test]
fn imported_labels_are_clipped_on_a_utf8_boundary() {
    let long = "z".repeat(MAX_LABEL_BYTES * 2);
    let attached = TimingReport::from_root(TimingNode::new(long, Duration::from_millis(1)));
    let multibyte_attached = TimingReport::from_root(TimingNode::new(
        "ø".repeat(MAX_LABEL_BYTES),
        Duration::from_millis(1),
    ));

    let (result, report) = Timing::record("caller", |root| {
        root.child("call").run(|active| {
            active.attach(Some(attached));
            ok(())
        })?;
        root.child("multibyte").run(|active| {
            active.attach(Some(multibyte_attached));
            ok(())
        })
    });

    assert_eq!(result, Ok(()));
    let root = report.root().expect("a measured report has a root");
    let ascii = &root.children()[0].children()[0];
    assert_eq!(ascii.name().len(), MAX_LABEL_BYTES);
    assert!(ascii.is_incomplete());
    let multibyte = &root.children()[1].children()[0];
    assert_eq!(multibyte.name().len(), MAX_LABEL_BYTES);
    assert_eq!(multibyte.name().chars().count(), MAX_LABEL_BYTES / 2);
    assert!(multibyte.is_incomplete());
    assert!(root.is_incomplete());
}

#[test]
fn attachment_consumes_the_assembled_tree_remaining_budget() {
    let mut remote_children = Vec::new();
    for index in 0..(MAX_NODES - 1) {
        remote_children.push(TimingNode::new(
            format!("remote.{index}"),
            Duration::from_nanos(1),
        ));
    }
    let attached = TimingReport::from_root(
        TimingNode::new("remote.execute", Duration::from_millis(1)).with_children(remote_children),
    );
    assert_eq!(attached.node_count(), MAX_NODES);

    let mut after_attach = 0_u32;
    let (result, report) = Timing::record("caller", |root| {
        root.child("call").run(|active| {
            active.attach(Some(attached));
            after_attach += 1;
            ok(())
        })?;
        root.child("after").run(|_| {
            after_attach += 1;
            ok(())
        })
    });

    assert_eq!(result, Ok(()));
    assert_eq!(after_attach, 2);
    assert_eq!(report.node_count(), MAX_NODES);
    let root = report.root().expect("a measured report has a root");
    assert!(root.is_incomplete());
    let call = &root.children()[0];
    assert!(call.is_incomplete());
    let grafted = &call.children()[0];
    assert!(grafted.is_incomplete());
    assert_eq!(grafted.children().len(), MAX_NODES - 3);
    // The budget is exhausted, so the later sibling is clipped as well.
    assert_eq!(child_names(root), ["call"]);
}

fn chain(
    scope: &TimingScope<'_, Active>,
    remaining: usize,
    attachment: &TimingReport,
) -> Result<(), TestError> {
    if remaining == 0 {
        scope.attach(Some(attachment.clone()));
        return ok(());
    }
    scope
        .child("level")
        .run(|child| chain(child, remaining - 1, attachment))
}

#[test]
fn attachment_cannot_exceed_the_depth_budget() {
    let attached =
        TimingReport::from_root(
            TimingNode::new("remote", Duration::from_millis(1)).with_children(vec![
                TimingNode::new("remote.inner", Duration::from_millis(1)).with_children(vec![
                    TimingNode::new("remote.deep", Duration::from_millis(1)),
                ]),
            ]),
        );

    let (result, report) = Timing::record("caller", |root| chain(root, 30, &attached));

    assert_eq!(result, Ok(()));
    assert_eq!(report.levels(), usize::from(MAX_DEPTH));
    let root = report.root().expect("a measured report has a root");
    assert!(root.is_incomplete());
    let grafted = deepest(root);
    assert_eq!(grafted.name(), "remote");
    assert!(grafted.is_incomplete());
    assert!(grafted.children().is_empty());
}

#[test]
fn synthetic_nodes_are_bounded_by_the_aggregate_limits() {
    let mut node = TimingNode::new("synthetic", Duration::ZERO);
    for index in 0..MAX_NODES {
        node.push_child(TimingNode::new(format!("n{index}"), Duration::ZERO));
    }
    assert_eq!(node.node_count(), MAX_NODES);
    assert_eq!(node.children().len(), MAX_NODES - 1);
    assert!(node.is_incomplete());
    assert!(!node.push_child(TimingNode::new("overflow", Duration::ZERO)));

    let mut deep = TimingNode::new("leaf", Duration::ZERO);
    for _ in 0..(MAX_DEPTH - 1) {
        deep = TimingNode::new("parent", Duration::ZERO).with_children(vec![deep]);
    }
    assert_eq!(deep.levels(), usize::from(MAX_DEPTH));
    assert!(!deep.is_incomplete());

    let rejected = TimingNode::new("parent", Duration::ZERO).with_children(vec![deep]);
    assert!(rejected.is_incomplete());
    assert!(rejected.children().is_empty());
    assert_eq!(rejected.levels(), 1);

    let report = TimingReport::from_root(node);
    assert_eq!(report.node_count(), MAX_NODES);
    assert!(report.is_incomplete());
}

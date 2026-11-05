//! B5 (#548): the close state machine, the generation CAS, the exec lease
//! and the reclamation brakes, with no runtime. Every state and event of the
//! protocol is exercised, and reclamation is keyed by machine and backend.
use layerfs_bridge::control::SdkValue;
use layerfs_control::common::json::parse;
use layerfs_control::common::outcome::View;
use layerfs_control::sandbox::lifecycle::{
    self, CloseDecision, CloseEvent, CloseRefusal, CloseState, ExecLease, ReclaimBackend,
    ReclaimDecision, ReclaimKey, ReclaimPolicy, SandboxFacts, SandboxState,
};
use layerfs_control::sandbox::record::SandboxRecord;

fn value(text: &str) -> SdkValue {
    parse(text).unwrap()
}

/// A process that has ended (and been reaped).
fn dead_pid() -> i64 {
    let mut child = std::process::Command::new("true").spawn().unwrap();
    child.wait().unwrap();
    i64::from(child.id())
}

#[test]
fn every_close_transition_is_explicit() {
    use CloseDecision as D;
    use CloseEvent as E;
    use CloseState as S;

    let request = |expected, current, live, expired| E::Request {
        expected,
        current,
        live_leases: live,
        expired_leases: expired,
    };
    // Every state crossed with every event. The answers are in state order:
    // Ready, Draining, Closed, Lost, Unresolved.
    let states = [S::Ready, S::Draining, S::Closed, S::Lost, S::Unresolved];
    let matrix: [(&str, E, [D; 5]); 7] = [
        (
            "request",
            request(4, 4, 0, 0),
            [
                D::Drain,
                D::Refuse(CloseRefusal::Illegal),
                D::Closed,
                D::Lost,
                D::Unresolved,
            ],
        ),
        (
            "stopped",
            E::Stopped,
            [
                D::Refuse(CloseRefusal::Illegal),
                D::Closed,
                D::Refuse(CloseRefusal::Illegal),
                D::Refuse(CloseRefusal::Illegal),
                D::Refuse(CloseRefusal::Illegal),
            ],
        ),
        (
            "provider_gone",
            E::ProviderGone,
            [
                D::Refuse(CloseRefusal::Illegal),
                D::Lost,
                D::Refuse(CloseRefusal::Illegal),
                D::Refuse(CloseRefusal::Illegal),
                D::Refuse(CloseRefusal::Illegal),
            ],
        ),
        (
            "unknown",
            E::Unknown,
            [
                D::Refuse(CloseRefusal::Illegal),
                D::Unresolved,
                D::Refuse(CloseRefusal::Illegal),
                D::Refuse(CloseRefusal::Illegal),
                D::Refuse(CloseRefusal::Illegal),
            ],
        ),
        (
            "panic",
            E::Panic,
            [
                D::Refuse(CloseRefusal::Illegal),
                D::Unresolved,
                D::Refuse(CloseRefusal::Illegal),
                D::Refuse(CloseRefusal::Illegal),
                D::Refuse(CloseRefusal::Illegal),
            ],
        ),
        (
            "failed",
            E::Failed,
            [
                D::Refuse(CloseRefusal::Illegal),
                D::Ready,
                D::Refuse(CloseRefusal::Illegal),
                D::Refuse(CloseRefusal::Illegal),
                D::Refuse(CloseRefusal::Illegal),
            ],
        ),
        (
            "replay",
            E::Replay { generation: 4 },
            [
                D::Refuse(CloseRefusal::Illegal),
                D::Refuse(CloseRefusal::Illegal),
                D::Closed,
                D::Lost,
                D::Unresolved,
            ],
        ),
    ];
    for (event_name, event, answers) in matrix {
        for (index, state) in states.iter().enumerate() {
            assert_eq!(
                lifecycle::close_transition(*state, event),
                answers[index],
                "{state:?} + {event_name} ({event:?})"
            );
        }
    }
    // The four request outcomes a Ready Sandbox can take.
    let ready = [
        (request(4, 4, 0, 0), D::Drain),
        (
            request(3, 4, 0, 0),
            D::Refuse(CloseRefusal::StaleGeneration),
        ),
        (request(4, 4, 1, 0), D::Refuse(CloseRefusal::LeaseActive)),
        (request(4, 4, 0, 1), D::Lost),
    ];
    for (event, expected) in ready {
        assert_eq!(
            lifecycle::close_transition(S::Ready, event),
            expected,
            "{event:?}"
        );
    }
}

#[test]
fn a_generation_and_its_lease_are_record_facts() {
    let record = value(r#"{"name":"s","id":"s","generation":7,"last_activity_monotonic":3}"#);
    assert_eq!(lifecycle::generation_of(&record), 7);
    assert_eq!(lifecycle::generation_of(&value(r#"{"name":"old"}"#)), 1);

    let record = lifecycle::with_lease(record, "lease-1", 1_000);
    assert_eq!(
        lifecycle::leases_of(&record),
        vec![ExecLease {
            id: "lease-1".to_owned(),
            deadline: 1_000,
        }]
    );
    let record = lifecycle::with_activity(record, 11);
    assert_eq!(record.get_int(lifecycle::LAST_ACTIVITY), Some(11));
    let record = lifecycle::without_lease(record, "lease-1");
    assert!(lifecycle::leases_of(&record).is_empty());
    assert!(record.get(lifecycle::LEASES).is_none());

    let record = lifecycle::with_lost(record, "an exec lease expired", 12);
    assert!(lifecycle::is_lost(&record));
    assert_eq!(
        record.get_text(lifecycle::CLOSE_REASON),
        Some("an exec lease expired")
    );
    assert_eq!(record.get_int(lifecycle::LAST_ACTIVITY), Some(12));
}

#[test]
fn a_sandbox_reads_busy_draining_and_lost() {
    let me = i64::from(std::process::id());
    let busy = SandboxRecord::from_record(
        "b",
        value(
            r#"{"name":"b","id":"b","generation":2,"exec_leases":[{"id":"lease-1","deadline":9999999999999}]}"#,
        ),
    );
    assert_eq!(busy.state(false), SandboxState::Busy);

    let draining = SandboxRecord::from_record(
        "d",
        value(&format!(
            r#"{{"name":"d","id":"d","generation":3,"op":{{"id":"op-1","name":"sandbox.close","phase":"draining","args":null,"owner":{{"pid":{me}}},"started":1,"updated":2,"uncertain":true}}}}"#
        )),
    );
    assert_eq!(draining.state(false), SandboxState::Draining);

    let lost = SandboxRecord::from_record(
        "l",
        value(
            r#"{"name":"l","id":"l","generation":4,"lost":true,"close_reason":"an exec lease expired"}"#,
        ),
    );
    assert_eq!(lost.state(false), SandboxState::Lost);
    assert_eq!(lost.generation(), 4);
    // Lost is observable, with its evidence, when the record is shown
    let shown = lost.to_sdk(View::List);
    assert_eq!(shown.get_text("state"), Some("lost"));
    assert_eq!(shown.get_int("generation"), Some(4));
    assert_eq!(
        shown.get_text("close_reason"),
        Some("an exec lease expired")
    );
}

#[test]
fn a_close_that_died_mid_drain_reads_unresolved() {
    let dead = dead_pid();
    let record = value(&format!(
        r#"{{"name":"c","id":"c","generation":6,"close_reason":"an operator asked to close it","op":{{"id":"op-00000000000000c1","name":"sandbox.close","phase":"draining","args":null,"owner":{{"pid":{dead}}},"started":1,"updated":2,"uncertain":true}}}}"#
    ));
    let crashed = SandboxRecord::from_record("c", record);
    assert_eq!(crashed.state(false), SandboxState::Unresolved);
    assert_eq!(
        crashed.to_record().get_text(lifecycle::CLOSE_REASON),
        Some("an operator asked to close it")
    );
    let shown = crashed.to_sdk(View::Full);
    assert_eq!(shown.get_text("state"), Some("unresolved"));
    assert_eq!(
        shown.get_text("close_reason"),
        Some("an operator asked to close it")
    );

    // A record written before the close marked itself uncertain is still
    // unresolved once its owner is gone: the outcome cannot be known.
    let legacy = value(&format!(
        r#"{{"name":"c","id":"c","generation":6,"op":{{"id":"op-00000000000000c1","name":"sandbox.close","phase":"draining","args":null,"owner":{{"pid":{dead}}},"started":1,"updated":2,"uncertain":false}}}}"#
    ));
    let crashed = SandboxRecord::from_record("c", legacy);
    assert_eq!(crashed.state(false), SandboxState::Unresolved);
}

#[test]
fn every_sandbox_state_is_explicit_and_reachable() {
    use layerfs_control::common::op::Standing;
    use SandboxState as S;

    let names: Vec<&str> = S::ALL.iter().map(|state| state.as_str()).collect();
    assert_eq!(
        names,
        [
            "creating",
            "provisioning",
            "starting",
            "ready",
            "busy",
            "draining",
            "closed",
            "lost",
            "failed",
            "unresolved",
        ]
    );

    let facts = |recorded, lost, close_crashed, op, gone, started, live_leases| SandboxFacts {
        recorded,
        lost,
        close_crashed,
        op,
        gone,
        started,
        live_leases,
    };
    let create = |phase| Some((Standing::Running, "sandbox.create", phase));
    let close = |phase| Some((Standing::Running, "sandbox.close", phase));

    // One mapping: every fact set reaches exactly the state it names.
    let cases = [
        (
            S::Creating,
            facts(None, false, false, create("claim"), false, false, 0),
        ),
        (
            S::Provisioning,
            facts(None, false, false, create("provisioning"), false, false, 0),
        ),
        (
            S::Starting,
            facts(None, false, false, create("start"), false, false, 0),
        ),
        (S::Ready, facts(None, false, false, None, false, true, 0)),
        (S::Busy, facts(None, false, false, None, false, true, 1)),
        (
            S::Draining,
            facts(None, false, false, close("draining"), false, true, 0),
        ),
        (S::Closed, facts(None, false, false, None, false, false, 0)),
        (
            S::Lost,
            facts(None, true, false, close("draining"), false, true, 0),
        ),
        (
            S::Failed,
            facts(Some(S::Failed), false, false, None, false, true, 0),
        ),
        (
            S::Unresolved,
            facts(None, false, true, close("draining"), false, true, 0),
        ),
    ];
    for (expected, facts) in cases {
        assert_eq!(lifecycle::sandbox_state(facts), expected, "{expected:?}");
    }

    // The precedence the mapping documents: Lost, then a crashed close,
    // then a live or uncertain intent, then a recorded state, then the
    // runtime and leases.
    assert_eq!(
        lifecycle::sandbox_state(facts(
            Some(S::Ready),
            true,
            true,
            close("draining"),
            true,
            true,
            1
        )),
        S::Lost
    );
    assert_eq!(
        lifecycle::sandbox_state(facts(Some(S::Ready), false, true, None, false, true, 1)),
        S::Unresolved
    );
    // a live intent describes now, before an older recorded state
    assert_eq!(
        lifecycle::sandbox_state(facts(
            Some(S::Ready),
            false,
            false,
            close("draining"),
            false,
            true,
            0
        )),
        S::Draining
    );
    assert_eq!(
        lifecycle::sandbox_state(facts(Some(S::Failed), false, false, None, true, true, 1)),
        S::Failed
    );
    // the machine's answer that it no longer runs is Closed, whatever the
    // leases say
    assert_eq!(
        lifecycle::sandbox_state(facts(None, false, false, None, true, true, 1)),
        S::Closed
    );
}

#[test]
fn old_records_default_to_the_derived_enum() {
    use SandboxState as S;
    // A layout 3 record carries no `state`; the facts decide, exactly as
    // before, and the answer is the enum.
    let ready = SandboxRecord::from_record("r", value(r#"{"name":"r","id":"r"}"#));
    assert_eq!(ready.state(false), S::Ready);
    assert_eq!(ready.to_sdk(View::List).get_text("state"), Some("ready"));

    let closed = SandboxRecord::from_record("c", value(r#"{"name":"c"}"#));
    assert_eq!(closed.state(false), S::Closed);

    let me = i64::from(std::process::id());
    let creating = SandboxRecord::from_record(
        "n",
        value(&format!(
            r#"{{"name":"n","op":{{"id":"op-1","name":"sandbox.create","phase":"claim","args":null,"owner":{{"pid":{me}}},"started":1,"updated":2,"uncertain":false}}}}"#
        )),
    );
    assert_eq!(creating.state(false), S::Creating);

    // An explicit state on a record written before layout 3 is read through
    // the same enum, including the old names; an unknown one falls back to
    // the facts.
    for (name, expected) in [
        ("creating", S::Creating),
        ("provisioning", S::Provisioning),
        ("starting", S::Starting),
        ("running", S::Ready),
        ("ready", S::Ready),
        ("busy", S::Busy),
        ("closing", S::Draining),
        ("draining", S::Draining),
        ("gone", S::Closed),
        ("closed", S::Closed),
        ("lost", S::Lost),
        ("failed", S::Failed),
        ("unknown", S::Unresolved),
        ("unresolved", S::Unresolved),
    ] {
        let record = SandboxRecord::from_record(
            "r",
            value(&format!(r#"{{"name":"r","id":"r","state":"{name}"}}"#)),
        );
        assert_eq!(record.state(false), expected, "{name}");
        assert_eq!(
            record.to_sdk(View::List).get_text("state"),
            Some(expected.as_str()),
            "{name}"
        );
    }
    let unknown = SandboxRecord::from_record(
        "u",
        value(r#"{"name":"u","id":"u","state":"no-such-state"}"#),
    );
    assert_eq!(unknown.state(false), S::Ready);
}

#[test]
fn reclamation_is_keyed_by_machine_and_backend_not_isolation() {
    let native = value(r#"{"machine":"box","backend":"native","isolation":"process"}"#);
    // the isolation class is not the key: the same backend on the same
    // machine reclaims the same way whatever its isolation says
    let container = value(r#"{"machine":"box","backend":"native","isolation":"container"}"#);
    assert_eq!(ReclaimKey::of(&native), ReclaimKey::of(&container));
    assert_eq!(ReclaimKey::of(&native).backend, ReclaimBackend::Native);

    // two backends on one machine are two keys (neither is an isolation
    // class); a different machine is another key
    let namespaces = value(r#"{"machine":"box","backend":"namespaces"}"#);
    assert_ne!(ReclaimKey::of(&native), ReclaimKey::of(&namespaces));
    let docker = value(r#"{"machine":"docker","backend":"docker"}"#);
    assert_eq!(ReclaimKey::of(&docker).backend, ReclaimBackend::Docker);
    assert_ne!(ReclaimKey::of(&native), ReclaimKey::of(&docker));
    let other = value(r#"{"machine":"other","backend":"native"}"#);
    assert_ne!(ReclaimKey::of(&native), ReclaimKey::of(&other));

    let shown = ReclaimKey::of(&native).to_sdk();
    assert_eq!(shown.get_text("machine"), Some("box"));
    assert_eq!(shown.get_text("backend"), Some("native"));

    // an older record that names only its runtime is classified too
    let old = value(r#"{"machine":"native","runtime":"namespaces"}"#);
    assert_eq!(ReclaimKey::of(&old).backend, ReclaimBackend::Namespaces);
}

#[test]
fn each_reclaim_brake_stops_the_destructive_path() {
    assert_eq!(
        ReclaimPolicy::new(true, false, false).decide(),
        ReclaimDecision::Reclaim
    );
    assert!(ReclaimPolicy::new(true, false, false).allows());
    assert_eq!(
        ReclaimPolicy::new(false, false, false).decide(),
        ReclaimDecision::Blocked("reclaim is disabled (LAYERFS_RECLAIM_ENABLED)")
    );
    assert_eq!(
        ReclaimPolicy::new(true, false, true).decide(),
        ReclaimDecision::Blocked("reclaim is frozen (LAYERFS_RECLAIM_FREEZE)")
    );
    assert_eq!(
        ReclaimPolicy::new(true, true, false).decide(),
        ReclaimDecision::DryRun
    );
    for policy in [
        ReclaimPolicy::new(false, false, false),
        ReclaimPolicy::new(true, true, false),
        ReclaimPolicy::new(true, false, true),
    ] {
        assert!(!policy.allows());
        assert!(policy.brake().is_some());
    }
    // the disabled and frozen brakes outrank a dry run
    assert_eq!(
        ReclaimPolicy::new(false, true, true).decide(),
        ReclaimDecision::Blocked("reclaim is disabled (LAYERFS_RECLAIM_ENABLED)")
    );
    assert_eq!(
        ReclaimPolicy::new(true, true, true).decide(),
        ReclaimDecision::Blocked("reclaim is frozen (LAYERFS_RECLAIM_FREEZE)")
    );
}

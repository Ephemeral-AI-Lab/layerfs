//! The one classifier of a connection that was not made, shared by the
//! Store doctor (`store.connection.<step>`) and every reach refusal
//! (`step`): a login refused is `authentication`, a missing database
//! `database`, no server answer `transport`; a status read's error names
//! the same step, and a URL it cannot read is `configuration`.
use layerfs_persistence::diagnostic::ConnectionStep;

#[test]
fn a_refused_connection_names_the_doctor_s_step() {
    let of = ConnectionStep::of_refused_connection;
    assert_eq!(of(Some("28P01")), ConnectionStep::Authentication);
    assert_eq!(of(Some("28000")), ConnectionStep::Authentication);
    assert_eq!(of(Some("3D000")), ConnectionStep::Database);
    assert_eq!(of(Some("57P03")), ConnectionStep::Transport);
    assert_eq!(of(None), ConnectionStep::Transport);
    for (step, word) in [
        (ConnectionStep::Configuration, "configuration"),
        (ConnectionStep::Transport, "transport"),
        (ConnectionStep::Tls, "tls"),
        (ConnectionStep::Authentication, "authentication"),
        (ConnectionStep::Database, "database"),
        (ConnectionStep::Schema, "schema"),
        (ConnectionStep::Identity, "identity"),
    ] {
        assert_eq!(step.name(), word);
        assert_eq!(step.id(), format!("store.connection.{word}"));
    }
}

#[test]
fn a_status_read_names_its_step() {
    let unread = layerfs_persistence::postgresql_access("not a url", "layerfs").unwrap_err();
    assert_eq!(unread.step(), ConnectionStep::Configuration, "{unread}");
    let nowhere = "postgres://lfs@127.0.0.1:1/db?sslmode=disable&connect_timeout=5";
    let unreached = layerfs_persistence::postgresql_access(nowhere, "layerfs").unwrap_err();
    assert_eq!(unreached.step(), ConnectionStep::Transport, "{unreached}");
}

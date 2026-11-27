//! #818 lane RH (M2 review): a read's one-statement history context keeps
//! today's refusal order on a corrupt Store. A marker row or a recorded
//! Branch row that does not decode refuses where the reads one by one meet
//! it (the marker step after the `path` argument, the answer's `at` after
//! the walk), never before the path check, the ownership check or the
//! tree's open. Each call is compared with the same call naming the
//! Commit by a prefix, which reads its context one by one. Needs
//! `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
use crate::adversary;

use adversary::{Service, TestStore};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};

const SEED: &[(&str, &[u8])] = &[("/seed", b"seed\n"), ("/dir/gone", b"gone\n")];

fn stat(service: &Service, snapshot: (&str, &str), path: &str) -> String {
    let answer: Result<SdkValue, SdkRefusal> = service.call(
        "fs.stat",
        vec![
            (snapshot.0, SdkValue::text(snapshot.1)),
            ("path", SdkValue::text(path)),
        ],
    );
    match answer {
        Ok(answer) => format!("ok {:?}", answer.get("kind")),
        Err(refusal) => format!("refused {} {}", refusal.code, refusal.message),
    }
}

/// One call by the one-statement route (the full id) and by the one-by-one
/// route (a prefix): the same outcome, which is returned.
fn alike(service: &Service, head: &str, path: &str) -> String {
    let by_id = stat(service, ("commit", head), path);
    let by_prefix = stat(service, ("commit", &head[..16]), path);
    assert_eq!(by_id, by_prefix, "{path}: by full id and by prefix");
    by_id
}

#[test]
fn a_corrupt_context_part_refuses_where_it_did() {
    let Some(store) = TestStore::postgres("context_corrupt_rh", SEED) else {
        return;
    };
    store.fork("work");
    let service = store.open();
    service
        .call(
            "fs.write",
            vec![
                ("branch", SdkValue::text("work")),
                ("path", SdkValue::text("/ctx.txt")),
                ("content", SdkValue::text("ctx\n")),
            ],
        )
        .expect("a Commit on work");
    let removed = service
        .call(
            "project.remove_file",
            vec![("path", SdkValue::text("/dir/gone"))],
        )
        .expect("a marker");
    assert_eq!(removed.get_text("kind"), Some("Removed"));
    let head = service
        .call(
            "fs.stat",
            vec![
                ("branch", SdkValue::text("work")),
                ("path", SdkValue::text("/")),
            ],
        )
        .expect("the head")
        .get("at")
        .and_then(|at| at.get_text("id"))
        .expect("the head Commit's id")
        .to_owned();
    let invalid = alike(&service, &head, "relative");
    assert!(invalid.starts_with("refused InvalidInput"), "{invalid}");
    assert!(alike(&service, &head, "/seed").starts_with("ok"));

    // A marker row that does not decode: the path argument still refuses
    // first, and a valid path refuses at the marker step, by every route.
    let mut admin = store.admin();
    admin
        .batch_execute(
            "ALTER TABLE path_removed DROP CONSTRAINT path_removed_frontier_serial_check;
             UPDATE path_removed SET frontier_serial = -1;",
        )
        .unwrap();
    assert_eq!(alike(&service, &head, "relative"), invalid);
    let marker = alike(&service, &head, "/seed");
    assert!(marker.starts_with("refused"), "{marker}");
    assert_eq!(stat(&service, ("branch", "work"), "relative"), invalid);
    assert_eq!(stat(&service, ("branch", "work"), "/seed"), marker);
    admin
        .batch_execute("UPDATE path_removed SET frontier_serial = 0;")
        .unwrap();

    // A recorded Branch row that does not decode (by id: the Branch is not
    // named, so only the head Commit's recorded Branch reads it): the path
    // refuses first, and a valid path refuses at the answer's `at`.
    admin
        .batch_execute(
            "ALTER TABLE branch DROP CONSTRAINT branch_name_check;
             UPDATE branch SET name = 'NOT A NAME' WHERE name = 'work';",
        )
        .unwrap();
    assert_eq!(alike(&service, &head, "relative"), invalid);
    let recorded = alike(&service, &head, "/seed");
    assert!(recorded.starts_with("refused"), "{recorded}");
    service.stop();
}

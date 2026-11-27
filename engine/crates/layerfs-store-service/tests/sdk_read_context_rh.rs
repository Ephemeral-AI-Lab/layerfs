//! #818 lane RH, R5: a read's history context is one statement on
//! PostgreSQL and the reads one by one on SQLite. Every refusal the context
//! decodes (no project, no Branch, an identity-shaped name, no Commit, no
//! Layer, then the path) and every answer's `at` must be the same on both
//! backends, in the same order. The PostgreSQL half needs
//! `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "support/round_trips.rs"]
mod rt;

use layerfs_protocol::provision::install::SdkValue;
use rt::{t, Store};

/// Each call's outcome, with ids masked: refusal code and message, or the
/// answer's kind with its `at` kind and Branch.
fn outcomes(store: &mut Store) -> Vec<String> {
    let source = store.dir.join("source");
    rt::build_tree(&source);
    store.init(&source, &mut |_, _| {});
    let opened = store.open("context");
    rt::fork_work(&opened);
    opened
        .call(
            "fs.write",
            vec![
                ("branch", t(rt::BRANCH)),
                ("path", t("/ctx.txt")),
                ("content", t("ctx\n")),
            ],
        )
        .unwrap();
    let stat = |fields: Vec<(&str, SdkValue)>| {
        let mut fields = fields;
        if !fields.iter().any(|(name, _)| *name == "path") {
            fields.push(("path", t("/")));
        }
        opened.call("fs.stat", fields)
    };
    let id = |fields: Vec<(&str, SdkValue)>| -> String {
        let answer = stat(fields).unwrap();
        answer
            .get("at")
            .and_then(|at| at.get_text("id"))
            .unwrap()
            .to_owned()
    };
    let head = id(vec![("branch", t(rt::BRANCH))]);
    let layer = id(vec![("branch", t("main"))]);
    // The same kind, one byte changed: no such row.
    let absent = |id: &str| {
        format!(
            "{}{}",
            &id[..64],
            if id.ends_with("00") { "01" } else { "00" }
        )
    };
    let cases: Vec<Vec<(&str, SdkValue)>> = vec![
        vec![("project", t("ghost")), ("branch", t(rt::BRANCH))],
        vec![("project", t("ghost")), ("branch", t("ghost"))],
        vec![("branch", t("ghost"))],
        vec![("branch", t("ghost")), ("path", t("relative"))],
        vec![("branch", t("deadbeef12"))],
        vec![("branch", t("deadbeef"))],
        vec![("commit", t(&absent(&head)))],
        vec![("commit", t(&absent(&layer)))],
        vec![("branch", t(&absent(&head)))],
        vec![("commit", t(&absent(&head))), ("path", t("relative"))],
        vec![("branch", t("main")), ("path", t("/missing"))],
        vec![("branch", t(rt::BRANCH)), ("path", t("relative"))],
        vec![("branch", t(rt::BRANCH))],
        vec![("branch", t("main"))],
        vec![("commit", t(&head))],
        vec![("commit", t(&layer))],
        vec![("branch", t(&head))],
        vec![("commit", t(&head[..12]))],
    ];
    let mask = |text: &str| text.replace(&head, "<head>").replace(&layer, "<layer>");
    let out = cases
        .into_iter()
        .map(|fields| match stat(fields) {
            Ok(answer) => {
                let at = answer.get("at").unwrap();
                format!(
                    "ok {} {:?}",
                    at.get_text("kind").unwrap(),
                    at.get_text("branch")
                )
            }
            Err(refusal) => {
                // Store-specific ids inside a message are masked.
                let refusal = mask(&refusal);
                match refusal.split_once(": ") {
                    Some((code, message)) if message.contains(" does not exist") => {
                        format!("refused {code}: {}", message.split(' ').next().unwrap())
                    }
                    _ => format!("refused {refusal}"),
                }
            }
        })
        .collect();
    opened.stop();
    out
}

#[test]
fn a_read_context_decodes_alike_on_both_backends() {
    let sqlite = outcomes(&mut Store::sqlite("context-sqlite"));
    for line in &sqlite {
        println!("READ_CONTEXT sqlite {line}");
    }
    let _turn = rt::postgresql_turn();
    let Some(mut store) = Store::postgresql("context") else {
        println!("NOT_RUN: no PostgreSQL test server");
        return;
    };
    let postgresql = outcomes(&mut store);
    assert_eq!(postgresql, sqlite, "the same outcomes in the same order");
}

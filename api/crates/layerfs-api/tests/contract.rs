//! The call surface contract (2.0). The specification's rows are
//! `core/docs/issues/call-surface-2-0/calls.json`, read here at test time;
//! `tests/contract_pending.json` lists what this build has not done yet.
//!
//! - Every call of calls.json is migrated (its table entry declares its
//!   forms, and they are the row's) or listed pending, never both and never
//!   neither; the table names no call the specification does not, unless
//!   pending lists it as an old name to be removed.
//! - One rule makes every answer: shaping each row's long example by the
//!   row's own keys gives exactly its long keys, and a short form inside
//!   its short keys, with no I/O.
//! - The codes, the fold of today's codes, the states and their companions
//!   and the progress event are the specification's.
//! - The three pilot calls answer in the row's shape against a real home.
//!
//! When the last package lands, set [`PENDING_MUST_BE_EMPTY`].
use layerfs_api::{json, Asked, Call, Form, Kind, Paging, Part, Place, Section};
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::{public_code, CODES, FOLDS};
use layerfs_home::state::{Reason, State, WORDS};
use std::path::{Path, PathBuf};

/// The one switch: `true` once every package has landed. Nothing may be
/// pending then: no unmigrated call, no old name, no difference from
/// calls.json.
const PENDING_MUST_BE_EMPTY: bool = false;

const SPEC: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../core/docs/issues/call-surface-2-0/calls.json"
);
const PENDING: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/contract_pending.json");

fn read(path: &str) -> SdkValue {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    json::parse(&text).unwrap_or_else(|e| panic!("{path}: {e:?}"))
}

fn at<'a>(value: &'a SdkValue, key: &str) -> &'a SdkValue {
    value.get(key).unwrap_or(&SdkValue::Null)
}

fn list(value: &SdkValue) -> &[SdkValue] {
    match value {
        SdkValue::List(items) => items,
        _ => &[],
    }
}

fn fields(value: &SdkValue) -> &[(String, SdkValue)] {
    match value {
        SdkValue::Map(fields) => fields,
        _ => &[],
    }
}

fn keys(value: &SdkValue) -> Vec<&str> {
    fields(value)
        .iter()
        .map(|(name, _)| name.as_str())
        .collect()
}

fn texts(value: &SdkValue) -> Vec<String> {
    list(value)
        .iter()
        .map(|item| match item {
            SdkValue::Text(text) => text.clone(),
            other => panic!("not text: {other:?}"),
        })
        .collect()
}

/// Every row of calls.json, in its order.
fn rows(spec: &SdkValue) -> Vec<&SdkValue> {
    list(at(spec, "families"))
        .iter()
        .flat_map(|family| list(at(family, "calls")))
        .collect()
}

fn name(row: &SdkValue) -> &str {
    row.get_text("call").unwrap()
}

/// `key` as this build spells it: calls.json's word unless pending's
/// `spec_sync.renames` renames it for `call`.
fn renamed(pending: &SdkValue, call: &str, key: &str) -> String {
    let renames = at(at(at(pending, "spec_sync"), "renames"), call);
    renames.get_text(key).unwrap_or(key).to_owned()
}

fn spelled(pending: &SdkValue, call: &str, keys: &SdkValue) -> Vec<String> {
    (texts(keys).iter())
        .map(|key| renamed(pending, call, key))
        .collect()
}

fn leak(words: Vec<String>) -> &'static [&'static str] {
    let words: Vec<&'static str> = words
        .into_iter()
        .map(|word| &*Box::leak(word.into_boxed_str()))
        .collect();
    Box::leak(words.into_boxed_slice())
}

/// The form `{ short: { always, sometimes }, long }` declares, with `parts`.
fn form_of(short: &SdkValue, long: &SdkValue, parts: Vec<(&'static str, Part)>) -> &'static Form {
    let form = Form::of(
        leak(texts(at(short, "always"))),
        leak(texts(at(short, "sometimes"))),
        leak(texts(long)),
    )
    .parts(Box::leak(parts.into_boxed_slice()));
    Box::leak(Box::new(form))
}

/// A row's own form: its keys, and its items' keys when it has rows.
fn row_form(row: &SdkValue) -> &'static Form {
    let item = at(row, "item");
    let parts = match (item.get("long"), item.get_text("scalar")) {
        (Some(long), _) => vec![(
            "items",
            Part::Rows(form_of(at(item, "short"), long, vec![])),
        )],
        (None, Some(word)) => vec![("items", Part::Scalars(leak(vec![word.to_owned()])[0]))],
        (None, None) => vec![],
    };
    form_of(at(row, "short"), at(row, "long"), parts)
}

fn asked(detail: bool, with: &[&str]) -> Asked {
    Asked {
        detail,
        with: with.iter().map(|name| (*name).to_owned()).collect(),
    }
}

/// Null, false, or an empty text, list or map: never in a short form
/// under a `sometimes` key.
fn empty(value: &SdkValue) -> bool {
    match value {
        SdkValue::Null | SdkValue::Bool(false) => true,
        SdkValue::Text(text) => text.is_empty(),
        SdkValue::List(items) => items.is_empty(),
        SdkValue::Map(fields) => fields.is_empty(),
        _ => false,
    }
}

/// Checks one short map against its form: only short keys, every `always`
/// key the long form held a value for, no empty `sometimes` key.
fn check_short(what: &str, form: &Form, short: &SdkValue, long: &SdkValue) {
    for (key, value) in fields(short) {
        let key = key.as_str();
        assert!(
            form.always.contains(&key) || form.sometimes.contains(&key),
            "{what}: `{key}` is not a short key"
        );
        if form.sometimes.contains(&key) {
            assert!(!empty(value), "{what}: `{key}` is empty in the short form");
        }
    }
    for key in form.always {
        if *key != "ok" && at(long, key) != &SdkValue::Null {
            assert!(short.get(key).is_some(), "{what}: `{key}` is always there");
        }
    }
}

#[test]
fn every_call_is_migrated_or_pending_and_the_table_names_no_other() {
    let spec = read(SPEC);
    let pending = read(PENDING);
    let named: Vec<&str> = rows(&spec).into_iter().map(name).collect();
    assert_eq!(named.len(), 85);
    let waiting = texts(at(&pending, "calls"));
    for call in &named {
        let migrated = layerfs_api::find(call).is_some_and(|entry| entry.form.is_some());
        let listed = waiting.iter().any(|waits| waits == call);
        assert!(
            migrated || listed,
            "{call}: neither migrated (its entry declares no form) nor listed in \
             contract_pending.json"
        );
        assert!(
            !(migrated && listed),
            "{call} is migrated; take it out of contract_pending.json"
        );
    }
    for waits in &waiting {
        assert!(
            named.contains(&waits.as_str()),
            "contract_pending.json lists `{waits}`, which calls.json does not name"
        );
    }
    // the table: a call of the specification, or an old name on its way out
    let old = at(&pending, "old_names");
    for entry in layerfs_api::calls() {
        let call = entry.name();
        assert!(
            named.contains(&call.as_str()) || old.get(&call).is_some(),
            "the table holds `{call}`: calls.json does not name it and \
             contract_pending.json does not list it as an old name to be removed"
        );
        if !named.contains(&call.as_str()) {
            assert!(entry.form.is_none(), "{call}: an old name declares no form");
        }
    }
    for (gone, replacement) in fields(old) {
        assert!(
            layerfs_api::find(gone).is_some(),
            "`{gone}` left the table; take it out of contract_pending.json"
        );
        assert!(!named.contains(&gone.as_str()), "{gone}");
        let SdkValue::Text(replacement) = replacement else {
            panic!("{gone}: name the call that replaces it")
        };
        assert!(
            named.contains(&replacement.as_str()),
            "{gone}: {replacement}"
        );
    }
    if PENDING_MUST_BE_EMPTY {
        assert!(waiting.is_empty(), "still pending: {waiting:?}");
        assert!(fields(old).is_empty(), "old names remain: {:?}", keys(old));
        for (what, ahead) in fields(at(&pending, "spec_sync")) {
            assert!(fields(ahead).is_empty(), "spec_sync.{what} is not empty");
        }
    }
}

fn kind_word(kind: Kind) -> &'static str {
    match kind {
        Kind::Name => "Name",
        Kind::Project => "Project",
        Kind::Text => "Text",
        Kind::Int => "Int",
        Kind::Bool => "Bool",
        Kind::Json => "Json",
        Kind::Image => "Image",
        Kind::Content => "Content",
        Kind::Size => "Size",
        Kind::Function => "Function",
    }
}

/// Checks a table entry's arguments against the row's `positionals` or
/// `options`: names, kinds and order; `optional`, `redact` and `shell`
/// where the row says them.
fn check_args(
    pending: &SdkValue,
    call: &str,
    what: &str,
    args: &[layerfs_api::Arg],
    row: &SdkValue,
) {
    let want: Vec<String> = (list(at(row, what)).iter())
        .map(|arg| renamed(pending, call, arg.get_text("name").unwrap()))
        .collect();
    let have: Vec<&str> = args.iter().map(|arg| arg.name).collect();
    assert_eq!(have, want, "{call}: {what}");
    for (arg, said) in args.iter().zip(list(at(row, what))) {
        let spot = format!("{call} {}", arg.name);
        assert_eq!(Some(kind_word(arg.kind)), said.get_text("kind"), "{spot}");
        if what == "positionals" {
            let optional = at(said, "optional") == &SdkValue::Bool(true);
            assert_eq!(arg.optional, optional, "{spot}: optional");
        }
        if let Some(SdkValue::Bool(redact)) = said.get("redact") {
            assert_eq!(arg.redacted(), *redact, "{spot}: redact");
        }
        if let Some(shell) = said.get("shell") {
            assert_eq!(
                arg.shell.map(SdkValue::text).unwrap_or_default(),
                *shell,
                "{spot}: shell"
            );
        }
    }
}

/// Checks a declared form against `{ short: { always, sometimes }, long }`.
fn check_form(
    pending: &SdkValue,
    call: &str,
    what: &str,
    form: &Form,
    short: &SdkValue,
    long: &SdkValue,
) {
    let spot = format!("{call}: {what}");
    assert_eq!(
        form.always,
        spelled(pending, call, at(short, "always")),
        "{spot} always"
    );
    assert_eq!(
        form.sometimes,
        spelled(pending, call, at(short, "sometimes")),
        "{spot} sometimes"
    );
    assert_eq!(form.long, spelled(pending, call, long), "{spot} long");
}

/// The form of the rows under `key` of `form`.
fn rows_of(form: &Form, key: &str) -> Option<&'static Form> {
    match form.part(key) {
        Some(Part::Rows(form)) => Some(form),
        _ => None,
    }
}

/// Checks one migrated entry against its row.
fn check_entry(pending: &SdkValue, entry: &Call, row: &SdkValue) {
    let call = entry.name();
    let form = entry.form.unwrap();
    assert_eq!(Some(entry.help), row.get_text("help"), "{call}: help");
    check_args(pending, &call, "positionals", entry.args, row);
    check_args(pending, &call, "options", entry.options, row);
    check_form(
        pending,
        &call,
        "result",
        form,
        at(row, "short"),
        at(row, "long"),
    );
    let item = at(row, "item");
    match (
        item.get("long"),
        item.get_text("scalar"),
        form.part("items"),
    ) {
        (Some(long), _, Some(Part::Rows(rows))) => {
            check_form(pending, &call, "item", rows, at(item, "short"), long)
        }
        (None, Some(word), Some(Part::Scalars(declared))) => assert_eq!(*declared, word, "{call}"),
        (None, None, None) => {}
        _ => panic!("{call}: its items are not the row's"),
    }
    assert_eq!(
        entry.paging.word(),
        row.get_text("paging"),
        "{call}: paging"
    );
    let flag = |key: &str| at(row, key) == &SdkValue::Bool(true);
    assert_eq!(entry.progress(), flag("progress"), "{call}: progress");
    assert_eq!(entry.quiet, flag("quiet"), "{call}: quiet");
    assert_eq!(
        Some(entry.profile.word()),
        row.get_text("profile"),
        "{call}: profile"
    );
    // what the shell reads from stdin, as the arguments say it
    let stdin = if (entry.args.iter().chain(entry.options)).any(|arg| arg.shell.is_some()) {
        Some("secret")
    } else if entry.args.iter().any(|arg| arg.kind == Kind::Content) {
        Some("content")
    } else if entry.args.iter().any(|arg| arg.kind == Kind::Json) {
        Some("json")
    } else {
        None
    };
    assert_eq!(stdin, row.get_text("stdin"), "{call}: stdin");
    // its sections: names in order, keys, and where a row's item takes them
    let with = at(row, "with");
    let declared: Vec<&str> = entry.with.iter().map(|section| section.name).collect();
    assert_eq!(declared, keys(with), "{call}: with");
    for section in entry.with {
        let said = at(with, section.name);
        let spot = format!("{call} --with {}", section.name);
        assert_eq!(section.keys, texts(at(said, "keys")), "{spot}: keys");
        match said.get_text("where") {
            Some("item") => assert_eq!(section.place, Place::Item, "{spot}"),
            // calls.json does not say whether a section answers under its
            // own name or flat at the top; the entry does
            _ => assert_ne!(section.place, Place::Item, "{spot}"),
        }
        if let Some(item) = said.get("item") {
            let rows = section.form.and_then(|form| rows_of(form, "items"));
            let rows = rows.unwrap_or_else(|| panic!("{spot}: its rows declare no form"));
            check_form(
                pending,
                &call,
                &spot,
                rows,
                at(item, "short"),
                at(item, "long"),
            );
        }
        if said.get("nodes").is_some() {
            check_nodes(pending, &call, section, at(said, "nodes"));
        }
    }
}

/// The node forms a tree section declares, by the word that names each:
/// the first `always` key of every form reachable from the section's.
fn nodes_of(form: &'static Form, found: &mut Vec<(&'static str, &'static Form)>) {
    for (_, part) in form.parts {
        if let Part::Rows(node) = part {
            let word = node.always[0];
            if !found.iter().any(|(held, _)| *held == word) {
                found.push((word, node));
                nodes_of(node, found);
            }
        }
    }
}

/// Checks a tree section's nodes against the row's `nodes`, or against
/// pending's `spec_sync.nodes` while this build is ahead of calls.json.
fn check_nodes(pending: &SdkValue, call: &str, section: &Section, said: &SdkValue) {
    let mut found = Vec::new();
    nodes_of(section.form.unwrap(), &mut found);
    let ahead = at(
        at(at(at(pending, "spec_sync"), "nodes"), call),
        section.name,
    );
    let matches = |node: &Form, want: &SdkValue| {
        node.always == texts(at(at(want, "short"), "always"))
            && node.sometimes == texts(at(at(want, "short"), "sometimes"))
            && node.long == texts(at(want, "long"))
    };
    if fields(ahead).is_empty() {
        for (word, node) in &found {
            assert!(
                matches(node, at(said, word)),
                "{call} --with {}: node `{word}` is not the row's",
                section.name
            );
        }
        return;
    }
    let listed: Vec<&str> = found.iter().map(|(word, _)| *word).collect();
    assert_eq!(
        listed,
        keys(ahead),
        "{call}: spec_sync.nodes names the nodes"
    );
    let mut differs = false;
    for (word, node) in &found {
        let want = at(ahead, word);
        assert!(
            matches(node, want),
            "{call} --with {}: node `{word}` is not what spec_sync.nodes says: {node:?}",
            section.name
        );
        differs |= said.get(word).is_none_or(|row| !matches(node, row));
    }
    assert!(
        differs,
        "calls.json now says {call}'s `{}` nodes as built; take them out of spec_sync.nodes",
        section.name
    );
}

#[test]
fn a_migrated_entry_declares_what_its_row_says() {
    let spec = read(SPEC);
    let pending = read(PENDING);
    let mut migrated = Vec::new();
    for row in rows(&spec) {
        let Some(entry) = layerfs_api::find(name(row)).filter(|entry| entry.form.is_some()) else {
            continue;
        };
        check_entry(&pending, entry, row);
        migrated.push(entry.name());
    }
    assert!(
        ["home.get", "home.log", "home.clear_log"]
            .iter()
            .all(|pilot| migrated.iter().any(|call| call == pilot)),
        "{migrated:?}"
    );
    // a rename this build is ahead with is still a difference from calls.json
    for (call, renames) in fields(at(at(&pending, "spec_sync"), "renames")) {
        let row = rows(&spec).into_iter().find(|row| name(row) == call);
        let row = row.unwrap_or_else(|| panic!("spec_sync.renames: no row {call}"));
        let said: Vec<String> = (list(at(row, "options")).iter())
            .map(|option| option.get_text("name").unwrap().to_owned())
            .chain(texts(at(row, "long")))
            .chain(texts(at(at(row, "item"), "long")))
            .collect();
        for (old, _) in fields(renames) {
            assert!(
                said.contains(old),
                "calls.json no longer says `{old}` in {call}; take it out of spec_sync.renames"
            );
        }
    }
    // the global options are the specification's
    let globals = list(at(&spec, "global_options"));
    for name in [layerfs_api::DETAIL, layerfs_api::WITH] {
        assert!(globals
            .iter()
            .any(|option| option.get_text("name") == Some(name)));
    }
    let sections = texts(at(&spec, "with_values"));
    for entry in layerfs_api::calls() {
        for section in entry.with {
            assert!(
                sections.iter().any(|word| word == section.name),
                "{}",
                section.name
            );
        }
    }
}

#[test]
fn every_row_s_long_example_shapes_to_its_long_and_short_keys() {
    // one rule, no I/O: all 85 rows, migrated or not
    let spec = read(SPEC);
    let mut seen = 0;
    for row in rows(&spec) {
        let call = name(row);
        let form = row_form(row);
        let example = at(row, "examples").get_text("long").unwrap();
        let full = json::parse(example).unwrap_or_else(|e| panic!("{call}: {e:?}"));
        let long = layerfs_api::shape::shape(call, form, &[], full.clone(), &asked(true, &[]));
        assert_eq!(keys(&long), texts(at(row, "long")), "{call}: long keys");
        let short = layerfs_api::shape::shape(call, form, &[], full, &asked(false, &[]));
        check_short(call, form, &short, &long);
        if short.get("ok").is_some() {
            assert_eq!(
                json::print(&short),
                r#"{"ok":true}"#,
                "{call}: `ok` only alone"
            );
        }
        if let Some(rows) = form.item() {
            let item_long = texts(at(at(row, "item"), "long"));
            for (long, short) in list(at(&long, "items"))
                .iter()
                .zip(list(at(&short, "items")))
            {
                assert_eq!(keys(long), item_long, "{call}: an item's long keys");
                check_short(&format!("{call} item"), rows, short, long);
            }
            assert_eq!(
                list(at(&long, "items")).len(),
                list(at(&short, "items")).len(),
                "{call}: the short form keeps every item"
            );
            // the list shape
            assert!(
                form.long.starts_with(&["items", "next_cursor"]) || !paged(row),
                "{call}"
            );
        }
        // the row's own short examples stay inside its short keys
        let sections = at(row, "with");
        for example in texts(at(at(row, "examples"), "short")) {
            let start = example
                .find('{')
                .unwrap_or_else(|| panic!("{call}: {example}"));
            let said = json::parse(&example[start..]).unwrap_or_else(|e| panic!("{call}: {e:?}"));
            if said.get("error").is_some() {
                continue;
            }
            for key in keys(&said) {
                let short = form.always.contains(&key) || form.sometimes.contains(&key);
                let section = sections.get(key).is_some()
                    || (fields(sections).iter())
                        .any(|(_, section)| texts(at(section, "keys")).iter().any(|k| k == key));
                assert!(
                    short || section,
                    "{call}: its example has `{key}`: {example}"
                );
            }
        }
        seen += 1;
    }
    assert_eq!(seen, 85);
}

fn paged(row: &SdkValue) -> bool {
    row.get_text("paging").is_some()
}

#[test]
fn the_shaping_rules_hold_on_a_made_up_answer() {
    const ROW: Form = Form::of(
        &["name", "ready"],
        &["missing", "state"],
        &["name", "ready", "missing", "state", "size"],
    );
    const LIST: Form = Form::of(
        &["items"],
        &["next_cursor"],
        &["items", "next_cursor", "skipped"],
    )
    .parts(&[("items", Part::Rows(&ROW))]);
    let sections = [
        Section::item("stats", &["count"]),
        Section::own("extra", &["a", "b"]),
    ];
    let row = |name: &str, ready: bool, extra: &[(&str, SdkValue)]| {
        let mut fields = vec![
            ("name".to_owned(), SdkValue::text(name)),
            ("ready".to_owned(), SdkValue::Bool(ready)),
        ];
        fields.extend(
            extra
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone())),
        );
        SdkValue::Map(fields)
    };
    let full = || {
        SdkValue::map([
            (
                "items",
                SdkValue::List(vec![
                    // `count` is answered whether or not `stats` was asked for
                    row(
                        "a",
                        true,
                        &[
                            ("state", SdkValue::text("ready")),
                            ("count", SdkValue::Int(3)),
                        ],
                    ),
                    row(
                        "b",
                        false,
                        &[("missing", SdkValue::List(vec![SdkValue::text("docker")]))],
                    ),
                ]),
            ),
            ("skipped", SdkValue::Int(0)),
            ("extra", SdkValue::map([("a", SdkValue::Int(1))])),
        ])
    };
    let shape = |detail, with: &[&str]| {
        json::print(&layerfs_api::shape::shape(
            "test",
            &LIST,
            &sections,
            full(),
            &asked(detail, with),
        ))
    };
    // short: an `always` key stays when false, a `sometimes` key only when
    // it is news (a ready state is not), a section only when asked for
    assert_eq!(
        shape(false, &[]),
        r#"{"items":[{"name":"a","ready":true},{"name":"b","ready":false,"missing":["docker"]}]}"#
    );
    // long: every declared key, in order, null when absent
    assert_eq!(
        shape(true, &[]),
        concat!(
            r#"{"items":[{"name":"a","ready":true,"missing":null,"state":"ready","size":null},"#,
            r#"{"name":"b","ready":false,"missing":["docker"],"state":null,"size":null}],"#,
            r#""next_cursor":null,"skipped":0}"#
        )
    );
    // a section an item takes goes flat into each item; one of its own
    // answers under its name; the long form has both even when absent
    assert_eq!(
        shape(false, &["stats", "extra"]),
        concat!(
            r#"{"items":[{"name":"a","ready":true,"count":3},"#,
            r#"{"name":"b","ready":false,"missing":["docker"]}],"extra":{"a":1}}"#
        )
    );
    assert!(shape(true, &["stats", "extra"]).ends_with(r#""skipped":0,"extra":{"a":1,"b":null}}"#));
    assert!(shape(true, &["stats"]).contains(r#""size":null,"count":3}"#));
    // an answer with nothing to say is `{"ok":true}`, and `ok` leaves once
    // anything else is there
    const DONE: Form = Form::of(&["ok"], &["commit"], &["ok", "commit", "count"]);
    let done = |fields: SdkValue, detail| {
        json::print(&layerfs_api::shape::shape(
            "test",
            &DONE,
            &[],
            fields,
            &asked(detail, &[]),
        ))
    };
    assert_eq!(
        done(SdkValue::map([("count", SdkValue::Int(2))]), false),
        r#"{"ok":true}"#
    );
    assert_eq!(
        done(SdkValue::map([("commit", SdkValue::text("12d1"))]), false),
        r#"{"commit":"12d1"}"#
    );
    assert_eq!(
        done(SdkValue::map([("count", SdkValue::Int(2))]), true),
        r#"{"ok":true,"commit":null,"count":2}"#
    );
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "does not declare")]
fn a_key_the_form_does_not_declare_fails_loudly_in_a_debug_build() {
    const DONE: Form = Form::of(&["ok"], &[], &["ok"]);
    let stray = SdkValue::map([("surprise", SdkValue::Int(1))]);
    layerfs_api::shape::shape("test", &DONE, &[], stray, &asked(false, &[]));
}

#[test]
fn the_codes_and_their_fold_are_the_specification_s() {
    let spec = read(SPEC);
    let pending = read(PENDING);
    assert_eq!(CODES.to_vec(), texts(at(&spec, "refusal_codes")));
    let shape = at(&spec, "refusal_shape");
    assert_eq!(CODES.to_vec(), texts(at(shape, "error")));
    // today's codes fold as calls.json says, plus what this build is ahead with
    let said = at(shape, "today_codes");
    let ahead = at(at(&pending, "spec_sync"), "folds");
    let mut publics = keys(said);
    for public in keys(ahead) {
        if !publics.contains(&public) {
            publics.push(public);
        }
    }
    let folding: Vec<&str> = FOLDS.iter().map(|(public, _)| *public).collect();
    assert_eq!(folding.len(), publics.len(), "{folding:?} {publics:?}");
    for (public, folded) in FOLDS {
        assert!(CODES.contains(&public), "{public}");
        let mut want = texts(at(said, public));
        for extra in texts(at(ahead, public)) {
            assert!(
                !want.contains(&extra),
                "calls.json now folds `{extra}`; take it out of spec_sync.folds"
            );
            want.push(extra);
        }
        assert_eq!(folded.to_vec(), want, "{public}");
        for code in folded {
            assert!(!CODES.contains(code), "{code} is public");
            assert_eq!(public_code(code), public);
        }
    }
    for code in CODES {
        assert_eq!(public_code(code), code);
    }
    assert_eq!(public_code("NobodyRaisesThis"), "Failed");
    // the refusal's keys
    let long = texts(at(shape, "long"));
    assert_eq!(long, ["error", "message", "fix", "detail"]);
    let refusal = layerfs_home::outcome::with_next(
        layerfs_home::outcome::with_fix(
            layerfs_home::outcome::refused("HeadMoved", "the head moved"),
            "layerfs branch get shop/api main",
        ),
        "commit",
        SdkValue::text("12d1c0ff"),
    );
    let refusal = layerfs_api::refusal::public(refusal);
    assert_eq!(
        json::print(&layerfs_api::refusal::value(&refusal, false)),
        concat!(
            r#"{"error":"Conflict","message":"the head moved","#,
            r#""fix":"layerfs branch get shop/api main","commit":"12d1c0ff"}"#
        )
    );
    assert_eq!(
        json::print(&layerfs_api::refusal::value(&refusal, true)),
        concat!(
            r#"{"error":"Conflict","message":"the head moved","#,
            r#""fix":"layerfs branch get shop/api main","commit":"12d1c0ff","#,
            r#""detail":{"code":"HeadMoved"}}"#
        )
    );
    let plain = layerfs_home::outcome::refused("InvalidInput", "no");
    assert_eq!(
        json::print(&layerfs_api::refusal::value(&plain, false)),
        r#"{"error":"InvalidInput","message":"no"}"#
    );
    assert_eq!(
        keys(&layerfs_api::refusal::value(&plain, true)),
        long.iter().map(String::as_str).collect::<Vec<_>>()
    );
}

#[test]
fn the_states_their_companions_and_the_progress_event_are_the_specification_s() {
    let spec = read(SPEC);
    assert_eq!(WORDS.to_vec(), texts(at(&spec, "states")));
    let companions = at(&spec, "state_companions");
    let reasons = at(at(at(companions, "broken"), "reason"), "one_of");
    let words: Vec<&str> = Reason::ALL.iter().map(|reason| reason.word()).collect();
    assert_eq!(words, texts(reasons));
    let blocker = State::broken(Reason::EngineDown, Some("open -a Docker"));
    let states = [
        State::Ready,
        State::Busy {
            name: "start".into(),
            phase: "boot".into(),
            started_ms: Some(1_794_993_419_000),
        },
        State::Stopped,
        blocker.blocks("machine", "dev").unwrap(),
        blocker.clone(),
        State::Unknown { observed_ms: None },
    ];
    for (state, word) in states.iter().zip(WORDS) {
        assert_eq!(state.word(), word);
        let written = state.write(SdkValue::map([("name", SdkValue::text("x"))]));
        // `name`, `state`, then exactly this state's companion keys
        let said = at(companions, word);
        assert_eq!(&keys(&written)[2..], keys(said), "{word}");
        for (key, rule) in fields(said) {
            let Some(inner) = rule.get("keys") else {
                continue;
            };
            let inner: Vec<String> = (texts(inner).into_iter())
                .map(|key| {
                    if key.starts_with('<') {
                        "machine".to_owned()
                    } else {
                        key
                    }
                })
                .collect();
            assert_eq!(keys(at(&written, key)), inner, "{word}.{key}");
        }
    }
    let busy = states[1].write(SdkValue::Map(Vec::new()));
    assert_eq!(
        json::print(&busy),
        r#"{"state":"busy","op":{"name":"start","phase":"boot","started_at":"2026-11-18T09:16:59Z"}}"#
    );
    // what needs a thing is blocked by the root of the block, never by a
    // thing that is itself blocked; a ready or unknown thing blocks nothing
    let below = states[3].blocks("store", "shop").unwrap();
    assert_eq!(
        json::print(&below.write(SdkValue::Map(Vec::new()))),
        r#"{"state":"blocked","blocked_by":{"machine":"dev","reason":"engine_down"}}"#
    );
    assert_eq!(
        State::Stopped.blocks("machine", "dev"),
        Some(State::blocked_by("machine", "dev", "stopped"))
    );
    assert!(State::Ready.blocks("machine", "dev").is_none());
    assert!(states[5].blocks("machine", "dev").is_none());
    // the progress event
    let mut seen = Vec::new();
    {
        let mut sink = |event: SdkValue| seen.push(event);
        let mut events = layerfs_home::events::Events::new(&mut sink);
        events.phase("copy", Some(3), Some(10), Vec::new());
    }
    let event = at(&seen[0], "progress");
    assert_eq!(keys(event), texts(at(&spec, "progress_event")));
    assert_eq!(
        json::print(event),
        r#"{"phase":"copy","done":3,"total":10}"#
    );
}

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-contract-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Result<SdkValue, String> {
    let args = SdkValue::Map(
        args.iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect(),
    );
    layerfs_api::call(Some(home), name, args, &mut |_| {})
        .map_err(|refusal| format!("{}: {}", refusal.code, refusal.message))
}

const DETAIL: (&str, SdkValue) = ("detail", SdkValue::Bool(true));

fn with(names: &[&str]) -> (&'static str, SdkValue) {
    let names = names.iter().map(|name| SdkValue::text(*name)).collect();
    ("with", SdkValue::List(names))
}

fn row<'a>(spec: &'a SdkValue, call: &str) -> &'a SdkValue {
    let row = rows(spec).into_iter().find(|row| name(row) == call);
    row.unwrap_or_else(|| panic!("no row {call}"))
}

/// Checks a real answer pair against the row: the long form has exactly
/// the long keys (and the nested maps the row's `nested` names theirs), the
/// short form only short keys, each with the long form's value.
fn check_answer(pending: &SdkValue, row: &SdkValue, short: &SdkValue, long: &SdkValue) {
    let call = name(row);
    assert_eq!(
        keys(long),
        spelled(pending, call, at(row, "long")),
        "{call}: long"
    );
    let form = layerfs_api::find(call).unwrap().form.unwrap();
    check_short(call, form, short, long);
    for (key, value) in fields(short) {
        if form.part(key).is_none() {
            assert_eq!(
                Some(value),
                long.get(key),
                "{call}: `{key}` differs between the forms"
            );
        }
    }
    for (key, nested) in fields(at(row, "nested")) {
        if let (Some(want), SdkValue::Map(_)) = (nested.get("long"), at(long, key)) {
            assert_eq!(keys(at(long, key)), texts(want), "{call}: `{key}`");
        }
    }
}

#[test]
fn home_get_answers_in_its_row_s_shape() {
    let spec = read(SPEC);
    let pending = read(PENDING);
    let said = row(&spec, "home.get");
    let home = tempdir("home-get");
    // no home yet
    let short = call(&home, "home.get", &[]).unwrap();
    let long = call(&home, "home.get", &[DETAIL]).unwrap();
    check_answer(&pending, said, &short, &long);
    assert_eq!(keys(&short), ["home", "state", "version", "reason", "fix"]);
    assert!(!home.exists());
    // a ready home
    call(&home, "home.init", &[]).unwrap();
    let short = call(&home, "home.get", &[]).unwrap();
    let long = call(&home, "home.get", &[DETAIL]).unwrap();
    check_answer(&pending, said, &short, &long);
    assert_eq!(keys(&short), ["home", "state", "version", "generation"]);
    assert_eq!(short.get_text("state"), Some("ready"));
    // a section answers only when asked for, in both forms
    assert!(long.get("tree").is_none() && long.get("platforms").is_none());
    call(&home, "store.create", &[("name", SdkValue::text("shop"))]).unwrap();
    let short = call(&home, "home.get", &[with(&["tree"])]).unwrap();
    let tree = at(&short, "tree");
    assert_eq!(keys(tree), ["machines", "stores", "buckets"]);
    // a ready machine and its ready sqlite Store say only their names
    let native = (list(at(tree, "machines")).iter())
        .find(|machine| machine.get_text("machine") == Some("native"))
        .unwrap_or_else(|| panic!("{tree:?}"));
    assert_eq!(
        json::print(native),
        r#"{"machine":"native","stores":[{"store":"shop"}]}"#
    );
    let long = call(&home, "home.get", &[DETAIL, with(&["tree"])]).unwrap();
    let mut top = spelled(&pending, "home.get", at(said, "long"));
    top.push("tree".to_owned());
    assert_eq!(keys(&long), top);
    let tree = at(&long, "tree");
    assert_eq!(keys(tree), texts(at(at(at(said, "with"), "tree"), "keys")));
    let machine = (list(at(tree, "machines")).iter())
        .find(|machine| machine.get_text("machine") == Some("native"))
        .unwrap();
    let entry = layerfs_api::find("home.get").unwrap();
    let nodes = entry.section("tree").unwrap().form.unwrap();
    assert_eq!(keys(machine), rows_of(nodes, "machines").unwrap().long);
    let store = &list(at(machine, "stores"))[0];
    assert_eq!(keys(store), rows_of(nodes, "stores").unwrap().long);
    assert_eq!(store.get_text("engine"), Some("sqlite"));
    assert_eq!(store.get_text("state"), Some("ready"));
    // an undeclared section is refused before anything is read
    let refused = call(&home, "home.get", &[with(&["live"])]).unwrap_err();
    assert_eq!(
        refused,
        "InvalidInput: home.get: `with` names `live`; this call takes platforms, tree"
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
#[cfg(feature = "machine")]
fn home_get_with_platforms_answers_its_rows_in_both_forms() {
    let spec = read(SPEC);
    let said = at(at(row(&spec, "home.get"), "with"), "platforms");
    let home = tempdir("home-platforms");
    let short = call(&home, "home.get", &[with(&["platforms"])]).unwrap();
    let long = call(&home, "home.get", &[DETAIL, with(&["platforms"])]).unwrap();
    assert!(!home.exists(), "the platforms need no home");
    let (short, long) = (at(&short, "platforms"), at(&long, "platforms"));
    assert_eq!(keys(long), texts(at(said, "keys")));
    assert_eq!(short.get("host"), long.get("host"));
    let rows = list(at(long, "items"));
    assert!(!rows.is_empty());
    assert_eq!(rows.len(), list(at(short, "items")).len());
    let item = at(said, "item");
    let always = texts(at(at(item, "short"), "always"));
    for (long, short) in rows.iter().zip(list(at(short, "items"))) {
        assert_eq!(keys(long), texts(at(item, "long")));
        // `ready` stays when false; `missing` only when something is
        let mut want = always.clone();
        if !list(at(long, "missing")).is_empty() {
            want.push("missing".to_owned());
        }
        assert_eq!(keys(short), want);
        for (key, nested) in fields(at(said, "nested")) {
            for inner in list(at(long, key)).iter().chain(Some(at(long, key))) {
                if let SdkValue::Map(_) = inner {
                    assert_eq!(keys(inner), texts(nested), "{key}");
                }
            }
        }
    }
}

#[test]
fn home_log_and_clear_log_answer_in_their_rows_shapes() {
    let spec = read(SPEC);
    let pending = read(PENDING);
    let home = tempdir("home-log");
    call(&home, "home.init", &[]).unwrap();
    for store in ["a", "b", "c"] {
        call(&home, "store.create", &[("name", SdkValue::text(store))]).unwrap();
    }
    call(&home, "store.get", &[("name", SdkValue::text("nope"))]).unwrap_err();
    let said = row(&spec, "home.log");
    let short = call(&home, "home.log", &[]).unwrap();
    let long = call(&home, "home.log", &[DETAIL]).unwrap();
    check_answer(&pending, said, &short, &long);
    assert_eq!(keys(&short), ["items"], "the last page has no cursor");
    let item_long = spelled(&pending, "home.log", at(at(said, "item"), "long"));
    let rows = list(at(&long, "items"));
    // newest first: the refusal, the three creates, the init
    assert_eq!(rows.len(), 5);
    assert!(rows.iter().all(|row| keys(row) == item_long));
    let newest = &list(at(&short, "items"))[0];
    assert_eq!(
        keys(newest),
        ["call", "outcome", "logged_at", "error"],
        "{newest:?}"
    );
    assert_eq!(newest.get_text("call"), Some("store.get"));
    assert_eq!(newest.get_text("outcome"), Some("refused"));
    assert_eq!(newest.get_text("error"), Some("NotFound"));
    // a call with no word of its own is `ok`: a record's own `kind` (the
    // new Store's `sqlite`) is not what happened
    let created = &list(at(&short, "items"))[1];
    assert_eq!(created.get_text("call"), Some("store.create"));
    assert_eq!(created.get_text("outcome"), Some("ok"));
    // pages: `limit` and the cursor of the page before
    let limit = ("limit", SdkValue::Int(2));
    let created = ("call", SdkValue::text("store.create"));
    let first = call(&home, "home.log", &[created.clone(), limit.clone()]).unwrap();
    assert_eq!(keys(&first), ["items", "next_cursor"]);
    assert_eq!(list(at(&first, "items")).len(), 2);
    let cursor = ("cursor", at(&first, "next_cursor").clone());
    let second = call(&home, "home.log", &[cursor.clone(), limit.clone()]).unwrap();
    assert_eq!(
        keys(&second),
        ["items"],
        "the cursor carried the filter: {second:?}"
    );
    assert_eq!(list(at(&second, "items")).len(), 1);
    // a cursor answers the call and the filters that made it, and no other
    let other = ("call", SdkValue::text("store.get"));
    let refused = call(&home, "home.log", &[cursor.clone(), other]).unwrap_err();
    assert!(refused.starts_with("InvalidInput: "), "{refused}");
    let bad = ("cursor", SdkValue::text("not-a-cursor"));
    let refused = call(&home, "home.log", &[bad]).unwrap_err();
    assert!(refused.starts_with("InvalidInput: "), "{refused}");
    // neither call wrote a line, and clear_log answers its row
    let said = row(&spec, "home.clear_log");
    let long = call(&home, "home.clear_log", &[DETAIL]).unwrap();
    assert_eq!(
        json::print(&long),
        r#"{"ok":true,"outcome":"cleared","line_count":5}"#
    );
    let short = call(&home, "home.clear_log", &[]).unwrap();
    assert_eq!(json::print(&short), r#"{"ok":true}"#);
    let long = call(&home, "home.clear_log", &[DETAIL]).unwrap();
    check_answer(&pending, said, &short, &long);
    assert_eq!(long.get_int("line_count"), Some(0));
    // a cursor from before the clear is stale, not an empty page
    let refused = call(&home, "home.log", &[cursor]).unwrap_err();
    assert!(refused.contains("stale"), "{refused}");
    // the old names are gone
    for gone in ["logs.list", "logs.reset"] {
        let refused = call(&home, gone, &[]).unwrap_err();
        assert_eq!(refused, format!("InvalidInput: no call named `{gone}`"));
    }
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn the_table_s_json_carries_each_migrated_entry_s_forms() {
    // what the Node, Python and MCP front ends and the generators read
    let table = json::parse(&layerfs_api::table_json()).unwrap();
    let mut migrated = 0;
    for call in list(&table) {
        let name = format!(
            "{}.{}",
            call.get_text("family").unwrap(),
            call.get_text("call").unwrap()
        );
        let entry = layerfs_api::find(&name).unwrap();
        let Some(form) = entry.form else {
            assert!(call.get("form").is_none(), "{name}");
            continue;
        };
        migrated += 1;
        let said = at(call, "form");
        assert_eq!(
            form.always,
            texts(at(at(said, "short"), "always")),
            "{name}"
        );
        assert_eq!(
            form.sometimes,
            texts(at(at(said, "short"), "sometimes")),
            "{name}"
        );
        assert_eq!(form.long, texts(at(said, "long")), "{name}");
        let sections: Vec<&str> = entry.with.iter().map(|section| section.name).collect();
        assert_eq!(sections, keys(at(said, "with")), "{name}");
        assert_eq!(entry.paging.word(), said.get_text("paging"), "{name}");
        assert_eq!(
            Some(entry.profile.word()),
            said.get_text("profile"),
            "{name}"
        );
        assert_eq!(
            entry.paging == Paging::None,
            at(said, "paging") == &SdkValue::Null
        );
    }
    // every migrated entry carries its form (no count is pinned during the
    // rollout)
    let entries = layerfs_api::calls()
        .into_iter()
        .filter(|entry| entry.form.is_some());
    assert_eq!(migrated, entries.count());
}

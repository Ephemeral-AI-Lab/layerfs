//! The 2.0 history calls' Store methods (call surface F-hist): Branch pages
//! by name with prefixes, a Commit list pinned to where it started, Commit
//! ids by prefix in one Store call, `commit.diff`'s default sides, its path
//! scope and pinned pages, a merge into an existing Branch and a publish of
//! a named Commit.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_protocol::provision::install::{SdkRefusal, SdkValue};
use std::fs;
use support::{items, write, Fixture};

fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}

fn seeded(label: &str) -> Fixture {
    Fixture::new(label, |source| {
        fs::create_dir_all(source.join("dir")).unwrap();
        fs::write(source.join("a.txt"), b"one\n").unwrap();
        fs::write(source.join("dir/b.txt"), b"bee\n").unwrap();
        3
    })
}

fn texts<'a>(value: &'a SdkValue, key: &str) -> Vec<&'a str> {
    items(value)
        .iter()
        .map(|item| item.get_text(key).unwrap_or("<none>"))
        .collect()
}

/// A Branch's head Commit, or main's head publication.
fn head(fixture: &Fixture, branch: &str) -> String {
    let listed = fixture.call("project.list_branches", vec![("name", t(branch))]);
    items(&listed)[0].get_text("commit").unwrap().to_owned()
}

fn refused(fixture: &Fixture, method: &str, fields: Vec<(&str, SdkValue)>) -> SdkRefusal {
    match fixture.try_call(method, fields) {
        Ok(value) => panic!("{method} answered {value:?}"),
        Err(refusal) => refusal,
    }
}

fn branch_page(fixture: &Fixture, fields: Vec<(&str, SdkValue)>) -> (Vec<String>, SdkValue) {
    let page = fixture.call("project.list_branches", fields);
    let names = texts(&page, "branch").into_iter().map(str::to_owned);
    (
        names.collect(),
        page.get("next").cloned().unwrap_or_default(),
    )
}

#[test]
fn branches_page_by_name_with_a_prefix_and_an_excluded_prefix() {
    let fixture = seeded("history-branch-pages");
    for name in ["dsh.a", "dsh.b", "dsh.c", "dsh.x.1", "other"] {
        fixture.fork(name);
    }
    // every Branch, main among them, in name order
    let (all, next) = branch_page(&fixture, vec![]);
    assert_eq!(all, ["dsh.a", "dsh.b", "dsh.c", "dsh.x.1", "main", "other"]);
    assert_eq!(next, SdkValue::Null);
    // a prefix, two at a time: each page starts after the last name
    let mut paged = Vec::new();
    let mut after = SdkValue::Null;
    for _ in 0..4 {
        let (names, next) = branch_page(
            &fixture,
            vec![
                ("prefix", t("dsh.")),
                ("after", after),
                ("limit", SdkValue::Int(2)),
            ],
        );
        assert!(names.len() <= 2, "{names:?}");
        paged.extend(names);
        if next == SdkValue::Null {
            break;
        }
        after = next;
    }
    assert_eq!(paged, ["dsh.a", "dsh.b", "dsh.c", "dsh.x.1"]);
    let (kept, _) = branch_page(
        &fixture,
        vec![("prefix", t("dsh.")), ("excludePrefix", t("dsh.x"))],
    );
    assert_eq!(kept, ["dsh.a", "dsh.b", "dsh.c"]);
    // an excluded prefix in the middle of the names is jumped over
    let (rest, _) = branch_page(&fixture, vec![("excludePrefix", t("dsh."))]);
    assert_eq!(rest, ["main", "other"]);
    for limit in [0, 1001] {
        let refusal = refused(
            &fixture,
            "project.list_branches",
            vec![("limit", SdkValue::Int(limit))],
        );
        assert_eq!(refusal.code, "InvalidInput", "{refusal:?}");
    }
    fixture.cleanup();
}

#[test]
fn a_commit_list_pages_from_the_commit_it_started_at() {
    let fixture = seeded("history-commit-pages");
    fixture.fork("feat");
    for n in 0..3 {
        fixture.apply("feat", vec![write(&format!("/f{n}.txt"), "x\n")]);
    }
    let page = |fields: Vec<(&str, SdkValue)>| {
        let mut fields = fields;
        fields.push(("branch", t("feat")));
        fixture.call("branch.list_commits", fields)
    };
    let first = page(vec![("limit", SdkValue::Int(2))]);
    let ids: Vec<String> = texts(&first, "commit")
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(ids.len(), 2);
    assert_eq!(ids[0], head(&fixture, "feat"));
    assert_eq!(first.get_text("next"), Some(ids[1].as_str()));
    let pin = first.get_text("at").unwrap().to_owned();
    assert_eq!(pin, ids[0]);
    assert!(ids.iter().all(|id| id.len() == 66), "{ids:?}");
    assert_eq!(texts(&first, "via"), ["file", "file"]);
    // the Branch moves on; the next page still lists the history pinned
    fixture.apply("feat", vec![write("/later.txt", "y\n")]);
    let second = page(vec![
        ("at", t(&pin)),
        ("after", t(&ids[1])),
        ("limit", SdkValue::Int(2)),
    ]);
    let later = texts(&second, "commit");
    assert!(!later.is_empty());
    assert!(
        !later.contains(&head(&fixture, "feat").as_str()),
        "{later:?}"
    );
    assert!(!later.iter().any(|id| ids.contains(&id.to_string())));
    assert_eq!(second.get_text("at"), Some(pin.as_str()));
    // `at` by a prefix starts the list at that Commit
    let from_second = page(vec![("at", t(&ids[1][..12])), ("limit", SdkValue::Int(1))]);
    assert_eq!(texts(&from_second, "commit"), [ids[1].as_str()]);
    // a pin no Commit has is NotFound's NoSuchCommit, not an empty page
    let gone = format!("{}{}", &pin[..2], "0".repeat(64));
    let refusal = refused(
        &fixture,
        "branch.list_commits",
        vec![
            ("branch", t("feat")),
            ("at", t(&gone)),
            ("after", t(&ids[1])),
        ],
    );
    assert_eq!(refusal.code, "NoSuchCommit", "{refusal:?}");
    // another Branch's Commit is not in this one's history
    fixture.fork("other");
    fixture.apply("other", vec![write("/o.txt", "o\n")]);
    let refusal = refused(
        &fixture,
        "branch.list_commits",
        vec![("branch", t("feat")), ("at", t(&head(&fixture, "other")))],
    );
    assert_eq!(refusal.code, "NotInHistory", "{refusal:?}");
    fixture.cleanup();
}

#[test]
fn a_commit_is_read_by_its_id_or_a_prefix_without_its_branch() {
    let fixture = seeded("history-commit-prefix");
    fixture.fork("feat");
    fixture.apply("feat", vec![write("/n.txt", "n\n")]);
    let id = head(&fixture, "feat");
    let get = |commit: &str| fixture.try_call("branch.get_commit", vec![("commit", t(commit))]);
    for given in [id.as_str(), &id[..8], &id[..20]] {
        let commit = get(given).unwrap();
        assert_eq!(commit.get_text("commit"), Some(id.as_str()), "{given}");
        assert_eq!(commit.get_text("branch"), Some("feat"));
        assert_eq!(commit.get_text("via"), Some("file"));
    }
    // too short, not hex, or naming nothing
    for (given, code) in [
        (&id[..7], "InvalidInput"),
        ("zzzzzzzzzz", "InvalidInput"),
        (&format!("{}{}", &id[..2], "000000")[..], "NoSuchCommit"),
    ] {
        let refusal = get(given).unwrap_err();
        assert_eq!(refusal.code, code, "{given}: {refusal:?}");
    }
    // main's publication reads the same way, as a Layer
    let main = head(&fixture, "main");
    let layer = get(&main[..10]).unwrap();
    assert_eq!(layer.get_text("commit"), Some(main.as_str()));
    assert_eq!(layer.get_text("via"), Some("init"));
    // a named Branch must hold it
    let refusal = refused(
        &fixture,
        "branch.get_commit",
        vec![("commit", t(&id)), ("branch", t("main"))],
    );
    assert_eq!(refusal.code, "NotInHistory", "{refusal:?}");
    fixture.cleanup();
}

fn diff(fixture: &Fixture, fields: Vec<(&str, SdkValue)>) -> SdkValue {
    fixture.call("commit.diff", fields)
}

#[test]
fn a_diff_compares_from_its_default_side_within_its_path() {
    let fixture = seeded("history-diff-sides");
    fixture.fork("feat");
    fixture.apply("feat", vec![write("/dir/new.txt", "new\n")]);
    let first = head(&fixture, "feat");
    fixture.apply("feat", vec![write("/a.txt", "two\n")]);
    let second = head(&fixture, "feat");
    let main = head(&fixture, "main");

    // a Branch: from its fork point
    let branch = diff(&fixture, vec![("ref", t("feat"))]);
    let paths = texts(&branch, "path");
    assert!(
        paths.contains(&"/a.txt") && paths.contains(&"/dir/new.txt"),
        "{paths:?}"
    );
    assert_eq!(branch.get_text("from"), Some(main.as_str()));
    assert_eq!(branch.get_text("to"), Some(second.as_str()));
    // within a path: that folder and what is under it
    let scoped = diff(&fixture, vec![("ref", t("feat")), ("path", t("/dir"))]);
    let paths = texts(&scoped, "path");
    assert!(paths.contains(&"/dir/new.txt"), "{paths:?}");
    assert!(paths.iter().all(|p| p.starts_with("/dir")), "{paths:?}");
    let file = diff(&fixture, vec![("ref", t("feat")), ("path", t("/a.txt"))]);
    assert_eq!(texts(&file, "path"), ["/a.txt"]);
    assert_eq!(texts(&file, "change"), ["modified"]);
    // a Commit: from its parent
    let commit = diff(&fixture, vec![("ref", t(&second[..12]))]);
    let paths = texts(&commit, "path");
    assert!(
        paths.contains(&"/a.txt") && !paths.contains(&"/dir/new.txt"),
        "{paths:?}"
    );
    assert_eq!(commit.get_text("from"), Some(first.as_str()));
    // a Branch's first Commit: from the base it was saved on
    let base = diff(&fixture, vec![("ref", t(&first))]);
    let paths = texts(&base, "path");
    assert!(
        paths.contains(&"/dir/new.txt") && !paths.contains(&"/a.txt"),
        "{paths:?}"
    );
    assert_eq!(base.get_text("from"), Some(main.as_str()));
    // main's first publication: from the empty tree, every path added
    let empty = diff(&fixture, vec![("ref", t("main"))]);
    assert_eq!(empty.get("from"), Some(&SdkValue::Null));
    assert_eq!(empty.get("fromRoot"), Some(&SdkValue::Null));
    let paths = texts(&empty, "path");
    assert!(
        paths.contains(&"/a.txt") && paths.contains(&"/dir/b.txt"),
        "{paths:?}"
    );
    assert!(texts(&empty, "change").iter().all(|c| *c == "added"));
    let empty_dir = diff(&fixture, vec![("ref", t("main")), ("path", t("/dir"))]);
    assert_eq!(texts(&empty_dir, "path"), ["/dir/b.txt"]);
    // an explicit `from`, a Branch name first
    let named = diff(&fixture, vec![("ref", t("feat")), ("from", t("main"))]);
    assert_eq!(texts(&named, "path"), texts(&branch, "path"));
    // lines only when asked
    let lines = diff(
        &fixture,
        vec![
            ("ref", t("feat")),
            ("path", t("/a.txt")),
            ("hunks", SdkValue::Bool(true)),
        ],
    );
    assert_ne!(items(&lines)[0].get("hunks"), Some(&SdkValue::Null));
    assert_eq!(items(&file)[0].get("hunks"), Some(&SdkValue::Null));
    fixture.cleanup();
}

#[test]
fn a_later_diff_page_compares_the_commits_the_first_page_pinned() {
    let fixture = seeded("history-diff-pins");
    fixture.fork("feat");
    fixture.apply(
        "feat",
        vec![write("/a.txt", "two\n"), write("/z.txt", "z\n")],
    );
    let first = diff(
        &fixture,
        vec![("ref", t("feat")), ("limit", SdkValue::Int(1))],
    );
    assert_eq!(items(&first).len(), 1);
    let next = first.get_text("next").expect("a next path").to_owned();
    let to = first.get_text("to").unwrap().to_owned();
    let from = first.get_text("from").unwrap().to_owned();
    // feat moves on; the pinned page does not see it
    fixture.apply("feat", vec![write("/b-late.txt", "late\n")]);
    let mut seen = texts(&first, "path")
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>();
    let mut after = next;
    for _ in 0..8 {
        let page = diff(
            &fixture,
            vec![
                ("pinned", SdkValue::Bool(true)),
                ("pinTo", t(&to)),
                ("pinFrom", t(&from)),
                ("after", t(&after)),
                ("limit", SdkValue::Int(1)),
            ],
        );
        seen.extend(texts(&page, "path").iter().map(|p| p.to_string()));
        match page.get_text("next") {
            Some(next) => after = next.to_owned(),
            None => break,
        }
    }
    assert!(seen.contains(&"/a.txt".to_owned()) && seen.contains(&"/z.txt".to_owned()));
    assert!(!seen.contains(&"/b-late.txt".to_owned()), "{seen:?}");
    let mut sorted = seen.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted, seen, "pages repeat or go back");
    fixture.cleanup();
}

fn merge(fixture: &Fixture, fields: Vec<(&str, SdkValue)>) -> Result<SdkValue, SdkRefusal> {
    fixture.try_call("branch.merge", fields)
}

#[test]
fn a_merge_moves_an_existing_branch_or_leaves_it_unchanged() {
    let fixture = seeded("history-merge-into");
    fixture.fork("left");
    fixture.fork("right");
    fixture.apply("right", vec![write("/r.txt", "r\n")]);
    let right = head(&fixture, "right");
    let into_left = || vec![("branch", t("left")), ("with", t("right"))];
    // left has nothing of its own: a fast-forward to right's head
    let forward = merge(&fixture, into_left()).unwrap();
    assert!(forward.get_bool("fastForward"), "{forward:?}");
    assert!(!forward.get_bool("unchanged"));
    assert_eq!(head(&fixture, "left"), right);
    assert_eq!(forward.get_text("commit"), Some(right.as_str()));
    // again: left already holds right, nothing changes
    let again = merge(&fixture, into_left()).unwrap();
    assert!(again.get_bool("unchanged"), "{again:?}");
    assert_eq!(again.get_text("commit"), Some(right.as_str()));
    assert_eq!(head(&fixture, "left"), right);
    // both move: a merge Commit on left, if left is where the caller read it
    fixture.apply("left", vec![write("/l.txt", "l\n")]);
    fixture.apply("right", vec![write("/r2.txt", "r2\n")]);
    let left = head(&fixture, "left");
    let mut stale = into_left();
    stale.push(("expected", t(&right)));
    let refusal = merge(&fixture, stale).unwrap_err();
    assert_eq!(refusal.code, "HeadMoved", "{refusal:?}");
    assert_eq!(refusal.detail.get_text("head"), Some(left.as_str()));
    assert_eq!(head(&fixture, "left"), left);
    let mut expected = into_left();
    expected.push(("expected", t(&left[..10])));
    let merged = merge(&fixture, expected).unwrap();
    assert!(!merged.get_bool("fastForward") && !merged.get_bool("unchanged"));
    let commit = head(&fixture, "left");
    assert_eq!(merged.get_text("commit"), Some(commit.as_str()));
    let read = fixture.call("branch.get_commit", vec![("commit", t(&commit))]);
    assert_eq!(read.get_text("via"), Some("merge"));
    assert_eq!(read.get_text("parent"), Some(left.as_str()));
    assert_eq!(
        read.get_text("secondParent"),
        Some(head(&fixture, "right").as_str())
    );
    // the old form: the result is a new Branch and left stays
    let mut into = into_left();
    into.push(("name", t("both")));
    merge(&fixture, into).unwrap();
    assert_eq!(head(&fixture, "left"), commit);
    // main moves only by publishing
    let refusal = merge(&fixture, vec![("branch", t("main")), ("with", t("left"))]).unwrap_err();
    assert_eq!(refusal.code, "InvalidInput", "{refusal:?}");
    fixture.cleanup();
}

#[test]
fn a_publish_names_the_commit_it_publishes() {
    let fixture = seeded("history-publish-at");
    fixture.fork("feat");
    fixture.apply("feat", vec![write("/one.txt", "1\n")]);
    let one = head(&fixture, "feat");
    fixture.apply("feat", vec![write("/two.txt", "2\n")]);
    let main = head(&fixture, "main");
    let publish = |fields: Vec<(&str, SdkValue)>| {
        let mut fields = fields;
        fields.push(("branch", t("feat")));
        fixture.try_call("branch.publish", fields)
    };
    // main's Layer is in feat's history but is no Commit to publish
    let refusal = publish(vec![("at", t(&main))]).unwrap_err();
    assert_eq!(refusal.code, "InvalidInput", "{refusal:?}");
    // another Branch's Commit is not in feat's history
    fixture.fork("other");
    fixture.apply("other", vec![write("/o.txt", "o\n")]);
    let refusal = publish(vec![("at", t(&head(&fixture, "other")))]).unwrap_err();
    assert_eq!(refusal.code, "NotInHistory", "{refusal:?}");
    // `expected` names main's head; a Branch's Commit is not one
    let refusal = publish(vec![("expected", t(&one))]).unwrap_err();
    assert_eq!(refusal.code, "InvalidInput", "{refusal:?}");
    let published = publish(vec![("at", t(&one[..12])), ("expected", t(&main[..12]))]).unwrap();
    let layer = published.get("publishedLayer").unwrap();
    assert_eq!(layer.get_text("sourceCommit"), Some(one.as_str()));
    assert_eq!(published.get_text("basedOn"), Some(main.as_str()));
    assert_eq!(head(&fixture, "main"), layer.get_text("id").unwrap());
    // the expected head is now stale: refused, naming main's head
    let refusal = publish(vec![("expected", t(&main))]).unwrap_err();
    assert_eq!(
        refusal.detail.get_text("head"),
        Some(head(&fixture, "main").as_str()),
        "{refusal:?}"
    );
    fixture.cleanup();
}

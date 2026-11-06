//! The `branch`, `commit` and `file` families (#533 step 7): Store calls on
//! one project's Branches, Commits and files, each translated to the Store
//! method that does it and run on the route its Store's records choose.
use crate::call::store_call;
use crate::resolve::target;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::context::Context;
use layerfs_home::outcome::{refused, Outcome};

/// The argument a scoped `branch.list` carries to its family: the Branch
/// namespace, which only a scope sets (the table has no such argument).
pub const NAMESPACE: &str = "namespace";

type Fields = Vec<(String, SdkValue)>;

fn arg(args: &SdkValue, name: &str) -> Option<SdkValue> {
    args.get(name)
        .filter(|v| !matches!(v, SdkValue::Null))
        .cloned()
}

/// `(to, value of from)` for each named argument the call carries.
fn take(args: &SdkValue, names: &[(&str, &str)]) -> Fields {
    names
        .iter()
        .filter_map(|(from, to)| arg(args, from).map(|v| ((*to).to_owned(), v)))
        .collect()
}

fn text<'a>(args: &'a SdkValue, name: &str) -> &'a str {
    args.get_text(name).unwrap_or("")
}

/// Runs Store method `op` with `fields` on the call's project.
fn on_project(context: &mut Context<'_>, args: &SdkValue, op: &str, fields: Fields) -> Outcome {
    let target = target(&context.home, text(args, "project"))?;
    store_call(context, &target, op, fields)
}

const PAGE: [(&str, &str); 2] = [("page_size", "pageSize"), ("page_pos", "pagePos")];

/// A diff's page and whether it carries hunks (`paths_only` is its opposite).
fn diff_options(args: &SdkValue) -> Fields {
    let mut fields = take(
        args,
        &[
            ("page_size", "pageSize"),
            ("page_pos", "pagePos"),
            ("cursor", "cursor"),
        ],
    );
    if let Some(SdkValue::Bool(paths_only)) = args.get("paths_only") {
        fields.push(("hunks".to_owned(), SdkValue::Bool(!paths_only)));
    }
    fields
}

/// The snapshot a read names: its Branch, or the Commit when given.
fn at(args: &SdkValue) -> Fields {
    take(args, &[("branch", "branch"), ("commit", "commit")])
}

/// Runs one call of the three families.
pub fn run(context: &mut Context<'_>, family: &str, call: &str, args: &SdkValue) -> Outcome {
    match (family, call) {
        ("branch", "list") => match args.get_text(NAMESPACE) {
            Some(namespace) => branches_in(context, args, namespace),
            None => on_project(context, args, "project.list_branches", take(args, &PAGE)),
        },
        ("branch", "get") => {
            let fields = take(args, &[("branch", "name")]);
            let page = on_project(context, args, "project.list_branches", fields)?;
            first(page, "NoSuchBranch", "Branch", text(args, "branch"))
        }
        ("branch", "fork") => {
            let from = arg(args, "from_commit").or_else(|| arg(args, "from_branch"));
            let mut fields = take(args, &[("name", "name")]);
            fields.extend(from.map(|from| ("from".to_owned(), from)));
            on_project(context, args, "branch.fork", fields)
        }
        ("branch", "merge") => {
            let fields = take(
                args,
                &[
                    ("a", "branch"),
                    ("b", "with"),
                    ("name", "name"),
                    ("message", "message"),
                    ("units", "units"),
                ],
            );
            on_project(context, args, "branch.merge", fields)
        }
        ("branch", "publish") => on_project(
            context,
            args,
            "branch.publish",
            take(args, &[("branch", "branch"), ("commit", "commit")]),
        ),
        ("branch", "diff") => {
            let mut fields = take(args, &[("from_branch", "from"), ("to_branch", "to")]);
            fields.extend(diff_options(args));
            on_project(context, args, "branch.diff_branches", fields)
        }
        ("commit", "list") => {
            let mut fields = take(args, &[("branch", "branch")]);
            fields.extend(take(args, &PAGE));
            on_project(context, args, "branch.list_commits", fields)
        }
        ("commit", "get") => commit_get(context, args),
        ("commit", "diff") => {
            let mut fields = take(args, &[("from_commit", "from"), ("to_commit", "to")]);
            fields.extend(diff_options(args));
            on_project(context, args, "branch.diff_commits", fields)
        }
        ("file", _) => file(context, call, args),
        _ => Err(refused("InvalidInput", format!("no call {family}.{call}"))),
    }
}

/// The one item of a list page whose `name` or `id` is `wanted`; else the
/// refusal `code` (S §2.9 keeps 1.1.0's `NoSuch*` codes).
fn first(page: SdkValue, code: &str, what: &str, wanted: &str) -> Outcome {
    let items = match page.get("items") {
        Some(SdkValue::List(items)) => items.clone(),
        _ => Vec::new(),
    };
    items
        .into_iter()
        .find(|item| item.get_text("name") == Some(wanted) || item.get_text("id") == Some(wanted))
        .ok_or_else(|| refused(code, format!("no {what} {wanted}")))
}

/// The items of one list page and the position of the next page, `None`
/// after the last. A next position that does not move on is `Failed`:
/// the list cannot go on, and it is not asked again.
fn page_of(page: &SdkValue, pos: i64) -> Result<(Vec<SdkValue>, Option<i64>), SdkRefusal> {
    let items = match page.get("items") {
        Some(SdkValue::List(items)) => items.clone(),
        _ => Vec::new(),
    };
    match page.get("nextPagePos") {
        Some(SdkValue::Int(next)) if *next > pos => Ok((items, Some(*next))),
        Some(SdkValue::Int(_)) => Err(refused(
            "Failed",
            format!("a list made no progress at page position {pos}"),
        )),
        _ => Ok((items, None)),
    }
}

/// `commit.get`: the Commit, found by paging the Branch's Commits newest
/// first, each page from where the last one ended, to the end.
fn commit_get(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let target = target(&context.home, text(args, "project"))?;
    let wanted = text(args, "commit");
    let mut pos = 0i64;
    loop {
        let fields = vec![
            ("branch".to_owned(), SdkValue::text(text(args, "branch"))),
            ("pageSize".to_owned(), SdkValue::Int(1000)),
            ("pagePos".to_owned(), SdkValue::Int(pos)),
        ];
        let page = store_call(context, &target, "branch.list_commits", fields)?;
        let (items, next) = page_of(&page, pos)?;
        let found = items.into_iter().find(|item| {
            item.get_text("id")
                .is_some_and(|id| id == wanted || (wanted.len() >= 8 && id.starts_with(wanted)))
        });
        if let Some(found) = found {
            return Ok(found);
        }
        match next {
            Some(next) => pos = next,
            None => {
                return Err(refused(
                    "NoSuchCommit",
                    format!("no Commit {wanted} on Branch {}", text(args, "branch")),
                ))
            }
        }
    }
}

/// A scoped `branch.list`: the Branches named `<namespace>.<name>`, named
/// relative to the namespace, one page (`page_size`, `page_pos`) of them.
/// The project's Branches are read page by page to the end, or until the
/// page is full; only the requested page is kept.
fn branches_in(context: &mut Context<'_>, args: &SdkValue, namespace: &str) -> Outcome {
    let size = args.get_int("page_size").unwrap_or(50);
    let skip = args.get_int("page_pos").unwrap_or(0);
    if !(1..=1000).contains(&size) || skip < 0 {
        return Err(refused(
            "InvalidInput",
            "page_size is in 1..=1000 and page_pos is not negative",
        ));
    }
    let target = target(&context.home, text(args, "project"))?;
    let prefix = format!("{namespace}.");
    let (mut pos, mut seen, mut kept, mut more) = (0i64, 0i64, Vec::new(), false);
    'pages: loop {
        let fields = vec![
            ("pageSize".to_owned(), SdkValue::Int(1000)),
            ("pagePos".to_owned(), SdkValue::Int(pos)),
        ];
        let page = store_call(context, &target, "project.list_branches", fields)?;
        let (items, next) = page_of(&page, pos)?;
        for item in items {
            let Some(name) = item.get_text("name").and_then(|n| n.strip_prefix(&prefix)) else {
                continue;
            };
            let name = SdkValue::text(name);
            if seen < skip {
                seen += 1;
            } else if kept.len() as i64 == size {
                more = true;
                break 'pages;
            } else {
                kept.push(layerfs_home::home::with_field(item, "name", name));
            }
        }
        match next {
            Some(next) => pos = next,
            None => break,
        }
    }
    let next = if more {
        SdkValue::Int(skip + kept.len() as i64)
    } else {
        SdkValue::Null
    };
    Ok(SdkValue::map([
        ("items", SdkValue::List(kept)),
        ("hasMore", SdkValue::Bool(more)),
        ("nextPagePos", next),
    ]))
}

/// One `file` call.
fn file(context: &mut Context<'_>, call: &str, args: &SdkValue) -> Outcome {
    let path = [("path", "path")];
    let message = [("message", "message")];
    let (op, fields): (&str, Fields) = match call {
        "read" => {
            let mut f = at(args);
            f.extend(take(
                args,
                &[
                    ("path", "path"),
                    ("start_line", "startLine"),
                    ("end_line", "endLine"),
                ],
            ));
            ("fs.read", f)
        }
        "stat" | "list" | "glob" | "grep" | "blame" => {
            let mut f = at(args);
            f.extend(take(args, &path));
            f.extend(take(
                args,
                &[("pattern", "pattern"), ("from_line", "fromLine")],
            ));
            f.extend(take(args, &PAGE));
            match call {
                "stat" => ("fs.stat", f),
                "list" => ("fs.list", f),
                "glob" => ("fs.glob", f),
                "grep" => ("fs.grep", f),
                _ => ("fs.blame", f),
            }
        }
        "diff" => {
            let side = |branch: &str, commit: &str, path: &str| {
                let at = arg(args, commit)
                    .or_else(|| arg(args, branch))
                    .unwrap_or_default();
                SdkValue::map([
                    ("commit", at),
                    ("path", arg(args, path).unwrap_or_default()),
                ])
            };
            let f = vec![
                (
                    "left".to_owned(),
                    side("left_branch", "left_commit", "left_path"),
                ),
                (
                    "right".to_owned(),
                    side("right_branch", "right_commit", "right_path"),
                ),
            ];
            ("fs.diff_files", f)
        }
        "write" => {
            let mut f = take(
                args,
                &[
                    ("branch", "branch"),
                    ("path", "path"),
                    ("message", "message"),
                    ("mode", "mode"),
                ],
            );
            match args.get("content") {
                Some(SdkValue::Bytes(bytes)) => {
                    f.push(("bytes".to_owned(), SdkValue::Bytes(bytes.clone())))
                }
                Some(content) => f.push(("content".to_owned(), content.clone())),
                None => {}
            }
            ("fs.write", f)
        }
        "edit" => {
            let mut f = take(
                args,
                &[
                    ("branch", "branch"),
                    ("path", "path"),
                    ("old", "oldText"),
                    ("new", "newText"),
                ],
            );
            f.extend(take(args, &message));
            ("fs.edit", f)
        }
        "rm" | "mkdir" => {
            let mut f = take(
                args,
                &[("branch", "branch"), ("path", "path"), ("mode", "mode")],
            );
            f.extend(take(args, &message));
            (if call == "rm" { "fs.rm" } else { "fs.mkdir" }, f)
        }
        "mv" | "cp" => {
            let mut f = take(
                args,
                &[
                    ("branch", "branch"),
                    ("from_path", "from"),
                    ("to_path", "to"),
                ],
            );
            f.extend(take(args, &message));
            (if call == "mv" { "fs.mv" } else { "fs.cp" }, f)
        }
        "apply" => {
            let mut f = take(args, &[("branch", "branch")]);
            f.extend(take(args, &message));
            f.push(("changes".to_owned(), changes(args.get("changes"))?));
            ("fs.apply", f)
        }
        _ => return Err(refused("InvalidInput", format!("no call file.{call}"))),
    };
    on_project(context, args, op, fields)
}

fn renamed(name: &str) -> &str {
    match name {
        "old" => "oldText",
        "new" => "newText",
        "from_path" => "from",
        "to_path" => "to",
        other => other,
    }
}

/// `file.apply`'s changes, from their snake_case fields to the Store's.
fn changes(value: Option<&SdkValue>) -> Result<SdkValue, SdkRefusal> {
    let Some(SdkValue::List(list)) = value else {
        return Err(refused(
            "InvalidInput",
            "`changes` must be a list of changes",
        ));
    };
    let mut out = Vec::with_capacity(list.len());
    for change in list {
        let SdkValue::Map(fields) = change else {
            return Err(refused("InvalidInput", "each change is a record"));
        };
        let mut mapped: Fields = Vec::with_capacity(fields.len());
        for (name, value) in fields {
            // the Store's own names are not a change's: every path field
            // must be one the folder scope checks
            if matches!(
                name.as_str(),
                "from" | "to" | "oldText" | "newText" | "bytes"
            ) {
                return Err(refused(
                    "InvalidInput",
                    format!(
                        "`{name}` is not a change field; a change uses op, path, from_path, \
                         to_path, content, old, new and mode"
                    ),
                ));
            }
            // write content may be bytes: the Store takes those as `bytes`
            if name == "content" && matches!(value, SdkValue::Bytes(_)) {
                mapped.push(("bytes".to_owned(), value.clone()));
            } else {
                mapped.push((renamed(name).to_owned(), value.clone()));
            }
        }
        out.push(SdkValue::Map(mapped));
    }
    Ok(SdkValue::List(out))
}

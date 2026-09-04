use super::workspace_common::{Case, Content, Entry, EntryKind};
use super::{dedup_workloads as d, Result};
use std::collections::BTreeMap;
pub(crate) const FAMILY: &str = "workspace_reliability";
pub(crate) const IDS: [&str; 28] = [
    "invalid-sdk-edit",
    "invalid-namespace",
    "lease-lifecycle",
    "open-writer-busy",
    "live-execution-busy",
    "candidate-failure-retry",
    "admission-batch-failure-retry",
    "final-publication-failure-retry",
    "published-presentation-failure",
    "dirty-end-discard",
    "dirty-net-zero",
    "short-spool-write",
    "deferred-nospace",
    "workload-cancel",
    "dirty-runtime-disconnect",
    "corrupt-descendant",
    "missing-descendant",
    "parallel-read-write",
    "shared-path-contention",
    "hardlink-alias",
    "symlink-semantics",
    "open-rename-unlink",
    "metadata-chmod",
    "metadata-mtime",
    "metadata-xattr",
    "exec-500",
    "repeat-publication",
    "sustained-600s",
];
pub(crate) fn cases() -> Vec<Case> {
    IDS.iter()
        .map(|&kind| Case {
            id: format!("workspace-{kind}-proof"),
            family: FAMILY,
            tier: 1,
            kind,
        })
        .collect()
}
pub(crate) fn resolve(id: &str) -> Result<Case> {
    cases()
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| "unknown reliability proof".into())
}
pub(crate) fn data(case: &Case, tag: &str, len: u64) -> Result<Content> {
    d::content(FAMILY, &case.id, 1, 0, tag, len)
}
pub(crate) fn fixture() -> Result<Vec<Entry>> {
    let mut rows = d::directories(&[
        "sentinels",
        "links",
        "work",
        "work/a",
        "work/b",
        "work/dir",
        "work/dir/child",
        "dest",
    ]);
    for i in 0..1000 {
        rows.push(Entry::file(
            format!("sentinels/f{i:04}.dat"),
            d::content(FAMILY, "fixture", 1, i, "bytes", 32768)?,
        ));
    }
    rows.push(Entry::hardlink("links/alias.dat", "sentinels/f0000.dat"));
    let links = [
        ("relative", "../sentinels/f0001.dat"),
        ("dangling", "absent"),
        ("cycle-a", "cycle-b"),
        ("cycle-b", "cycle-a"),
    ];
    let size: u64 = links.iter().map(|(_, target)| target.len() as u64).sum();
    for (path, target) in links {
        rows.push(Entry::symlink(format!("links/{path}"), target));
    }
    rows.push(Entry::file(
        "sentinels/balance.dat",
        d::content(
            FAMILY,
            "fixture",
            1,
            1000,
            "bytes",
            33554432 - 1000 * 32768 - 32768 - size,
        )?,
    ));
    Ok(rows)
}
fn add_file(
    map: &mut BTreeMap<String, Entry>,
    case: &Case,
    path: &str,
    tag: &str,
    len: u64,
) -> Result<()> {
    map.insert(path.into(), Entry::file(path, data(case, tag, len)?));
    Ok(())
}
fn remap(map: &mut BTreeMap<String, Entry>, from: &str, to: &str) {
    let moved: Vec<_> = map
        .keys()
        .filter(|p| p.as_str() == from || p.starts_with(&format!("{from}/")))
        .cloned()
        .collect();
    for old in moved {
        let mut e = map.remove(&old).unwrap();
        e.path = format!("{to}{}", &old[from.len()..]);
        map.insert(e.path.clone(), e);
    }
}
pub(crate) fn expected(case: &Case, state: &str, ordinal: u64) -> Result<Vec<Entry>> {
    let mut m: BTreeMap<_, _> = fixture()?
        .into_iter()
        .map(|e| (e.path.clone(), e))
        .collect();
    if state == "initial" {
        return Ok(m.into_values().collect());
    }
    if state == "prior" {
        add_file(&mut m, case, "work/a/prior.dat", "prior", 4096)?;
        return Ok(m.into_values().collect());
    }
    if case.kind == "dirty-net-zero" && state.starts_with("net-") {
        let entry = m.get_mut("sentinels/f0002.dat").unwrap();
        let EntryKind::File(base) = &entry.kind else {
            unreachable!()
        };
        let mut content = base.splice(0, 64, Content::Literal(vec![0x5a; 64]))?;
        if state == "net-append" {
            content = content.splice(32768, 0, Content::Literal(vec![0x33; 64]))?;
        }
        entry.kind = EntryKind::File(content);
        if matches!(state, "net-temp" | "net-moved") {
            add_file(
                &mut m,
                case,
                if state == "net-temp" {
                    "work/a/temp.dat"
                } else {
                    "work/b/temp.dat"
                },
                "temp",
                64,
            )?;
        }
        return Ok(m.into_values().collect());
    }
    if case.kind == "hardlink-alias" && state.starts_with("alias-") {
        m.insert(
            "work/a/alias".into(),
            Entry::hardlink("work/a/alias", "sentinels/f0000.dat"),
        );
        if state != "alias-created" {
            let e = m.get_mut("sentinels/f0000.dat").unwrap();
            let EntryKind::File(c) = &e.kind else {
                unreachable!()
            };
            e.kind = EntryKind::File(c.splice(0, 64, data(case, "alias-change", 64)?)?);
        }
        if matches!(state, "alias-moved" | "alias-unlinked") {
            remap(&mut m, "work/a", "dest/a");
        }
        if state == "alias-unlinked" {
            m.remove("links/alias.dat");
        }
        return Ok(m.into_values().collect());
    }
    if case.kind == "symlink-semantics" && state == "links-moved" {
        remap(&mut m, "links", "work/links");
        return Ok(m.into_values().collect());
    }
    match case.kind {
        "invalid-sdk-edit" | "invalid-namespace" => {
            add_file(&mut m, case, "work/a/prior.dat", "prior", 4096)?
        }
        "lease-lifecycle" | "dirty-net-zero" | "metadata-xattr" | "corrupt-descendant"
        | "missing-descendant" => (),
        "open-writer-busy" => add_file(&mut m, case, "work/a/writer.dat", "writer", 4096)?,
        "live-execution-busy" | "workload-cancel" | "dirty-runtime-disconnect" => {
            add_file(&mut m, case, "work/prefix.dat", "prefix", 4096)?
        }
        "candidate-failure-retry" => {
            add_file(&mut m, case, "work/a/one.dat", "one", 4096)?;
            add_file(&mut m, case, "work/b/two.dat", "two", 4096)?;
            m.insert(
                "work/b/link".into(),
                Entry::symlink("work/b/link", "../a/one.dat"),
            );
            remap(&mut m, "work/dir", "dest/dir");
        }
        "admission-batch-failure-retry" | "final-publication-failure-retry" => {
            for i in 0..17 {
                let path = format!(
                    "work/{}/dirty-{i:02}.dat",
                    if i % 2 == 0 { "a" } else { "b" }
                );
                add_file(
                    &mut m,
                    case,
                    &path,
                    &format!("dirty-{i}"),
                    if i < 15 { 1048576 } else { 65536 },
                )?;
            }
        }
        "published-presentation-failure" => {
            add_file(&mut m, case, "work/a/published.dat", "published", 4096)?
        }
        "dirty-end-discard" => add_file(
            &mut m,
            case,
            "work/a/result.dat",
            if state == "published" { "A" } else { "B" },
            4096,
        )?,
        "short-spool-write" | "deferred-nospace" => {
            add_file(&mut m, case, "work/a/prior.dat", "prior", 4096)?;
            add_file(&mut m, case, "work/b/fail.dat", "empty", 0)?;
        }
        "parallel-read-write" => {
            for w in 0..4 {
                let dir = format!("work/w{w}");
                m.insert(dir.clone(), Entry::directory(&dir));
                add_file(
                    &mut m,
                    case,
                    &format!("{dir}/final.dat"),
                    &format!("worker-{w}-cycle-15"),
                    4096,
                )?;
            }
        }
        "shared-path-contention" => {
            add_file(&mut m, case, "work/claim.dat", "claim", 0)?;
            add_file(&mut m, case, "work/shared.dat", "final", 4096)?;
        }
        "hardlink-alias" => {
            let entry = m.get_mut("sentinels/f0000.dat").unwrap();
            let EntryKind::File(base) = &entry.kind else {
                unreachable!()
            };
            entry.kind = EntryKind::File(base.splice(0, 64, data(case, "alias-change", 64)?)?);
            m.remove("links/alias.dat");
            remap(&mut m, "work/a", "dest/a");
            add_file(&mut m, case, "dest/a/alias", "replacement", 4096)?;
        }
        "symlink-semantics" => add_file(&mut m, case, "work/marker", "marker", 1)?,
        "open-rename-unlink" => add_file(&mut m, case, "work/a/target.dat", "new", 4096)?,
        "metadata-chmod" => {
            m.get_mut("sentinels/f0001.dat").unwrap().mode = 0o600;
            m.get_mut("work/a").unwrap().mode = 0o700;
        }
        "metadata-mtime" => {
            for path in ["sentinels/f0000.dat", "links/alias.dat", "work/a"] {
                let e = m.get_mut(path).unwrap();
                e.mtime_seconds = 1700000013;
                e.mtime_nanoseconds = 123456789;
            }
        }
        "exec-500" => add_file(
            &mut m,
            case,
            "work/result.dat",
            &format!("exec-{ordinal}"),
            4096,
        )?,
        "repeat-publication" => {
            for p in ["work/a/one", "work/b/two"] {
                add_file(&mut m, case, p, &format!("stage-{ordinal}"), 4096)?;
            }
        }
        "sustained-600s" => {
            if ordinal == 0 {
                return Err("sustained zero cycles".into());
            }
            for w in 0..2 {
                add_file(
                    &mut m,
                    case,
                    &format!("work/active{w}"),
                    &format!("active-{w}-{}", ordinal - 1),
                    4096,
                )?;
            }
            let mut bytes = format!("completed-cycles={ordinal}\n").into_bytes();
            bytes.resize(64, 0);
            m.insert(
                "work/cycle-result".into(),
                Entry::file("work/cycle-result", Content::Literal(bytes)),
            );
        }
        _ => return Err("unimplemented reliability expected state".into()),
    }
    Ok(m.into_values().collect())
}
pub(crate) fn self_check() -> Result<()> {
    let rows = cases();
    if rows.len() != 28
        || rows
            .iter()
            .map(|c| &c.id)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != 28
    {
        return Err("reliability membership".into());
    }
    let initial = fixture()?;
    if super::workspace_common::validate_entries(&initial)? != 33554432 {
        return Err("reliability exact initial bytes".into());
    }
    for c in rows {
        let expected = expected(&c, "done", 1)?;
        if super::workspace_common::validate_entries(&expected)? > 48 * 1048576 {
            return Err("reliability transient bound".into());
        }
    }
    Ok(())
}

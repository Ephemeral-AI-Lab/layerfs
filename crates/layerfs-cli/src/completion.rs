#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Completion {
    pub value: String,
    pub display: String,
}

pub(crate) fn complete(
    input: &str,
    cursor: usize,
    client: Option<&layerfs_sdk::Client>,
) -> crate::CliResult<Vec<Completion>> {
    let cursor = cursor.min(input.len());
    let before = &input[..cursor];
    let prefix = if before.chars().last().is_some_and(char::is_whitespace) {
        ""
    } else {
        before.split_whitespace().last().unwrap_or("")
    };
    let words = before.split_whitespace().collect::<Vec<_>>();
    if let Some(source) = branch_source(&words) {
        return client.map_or(Ok(Vec::new()), |client| {
            branch_completions(client, prefix, source)
        });
    }
    Ok([
        "db",
        "context",
        "layerstack",
        "branch",
        "workspace",
        "monitor",
        "query",
    ]
    .into_iter()
    .filter(|candidate| candidate.starts_with(prefix))
    .map(|value| Completion {
        value: value.to_owned(),
        display: value.to_owned(),
    })
    .collect())
}

#[derive(Clone, Copy)]
enum BranchSource {
    Authority,
    Receiver,
    Local,
}

fn branch_source(words: &[&str]) -> Option<BranchSource> {
    if matches!(words, ["branch", "pull", ..]) {
        Some(BranchSource::Authority)
    } else if matches!(words, ["branch", "push", ..] | ["layerstack", "add", ..]) {
        Some(BranchSource::Local)
    } else if matches!(words, ["workspace", "create", ..])
        || (matches!(words, ["branch", "fork", ..] | ["branch", "diff", ..])
            && words.contains(&"--branch"))
    {
        Some(BranchSource::Receiver)
    } else {
        None
    }
}

fn branch_completions(
    client: &layerfs_sdk::Client,
    prefix: &str,
    source: BranchSource,
) -> crate::CliResult<Vec<Completion>> {
    use layerfs_sdk::{BranchScope, Query, QueryItem, QueryKind};
    let kind = match source {
        BranchSource::Authority => QueryKind::AuthorityBranches,
        BranchSource::Receiver | BranchSource::Local => QueryKind::Branches,
    };
    let mut request = Query::new(kind);
    let mut completions = Vec::new();
    loop {
        let page = client.query(request.clone())?;
        let mut stack_names =
            std::collections::BTreeMap::<layerfs_sdk::LayerStackId, String>::new();
        for item in page.items {
            let branch = match item {
                QueryItem::Branch(branch) => branch,
                QueryItem::BranchScope(branch, scope)
                    if !matches!(source, BranchSource::Local)
                        || matches!(scope.scope, BranchScope::Local) =>
                {
                    branch
                }
                _ => continue,
            };
            let stack = match stack_names.get(&branch.layer_stack_id) {
                Some(name) => name.clone(),
                None => {
                    let name = stack_name(client, branch.layer_stack_id, source)?;
                    stack_names.insert(branch.layer_stack_id, name.clone());
                    name
                }
            };
            let qualified = format!("{stack}/{}", branch.name);
            let id = branch.id.to_string();
            if id.starts_with(prefix)
                || branch.name.as_str().starts_with(prefix)
                || qualified.starts_with(prefix)
            {
                completions.push(Completion {
                    value: id.clone(),
                    display: format!("{qualified} ({id})"),
                });
                if completions.len() == 512 {
                    return Ok(completions);
                }
            }
        }
        let Some(continuation) = page.continuation else {
            return Ok(completions);
        };
        request = request.after(continuation);
    }
}

fn stack_name(
    client: &layerfs_sdk::Client,
    id: layerfs_sdk::LayerStackId,
    source: BranchSource,
) -> crate::CliResult<String> {
    use layerfs_sdk::{Query, QueryItem, QueryKind};
    let mut query = Query::new(match source {
        BranchSource::Authority => QueryKind::AuthorityLayerStacks,
        BranchSource::Receiver | BranchSource::Local => QueryKind::LayerStacks,
    });
    loop {
        let page = client.query(query.clone())?;
        for item in page.items {
            match item {
                QueryItem::LayerStack(stack) if stack.id == id => {
                    return Ok(stack.name.to_string())
                }
                QueryItem::LayerStackScope(stack, _) if stack.id == id => {
                    return Ok(stack.name.to_string())
                }
                _ => {}
            }
        }
        let Some(continuation) = page.continuation else {
            return Ok("?".to_owned());
        };
        query = query.after(continuation);
    }
}

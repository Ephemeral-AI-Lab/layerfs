use crate::{CliEvent, CliResult, CommandResult};
use std::io::Write;

pub(crate) fn render(event: &CliEvent, json: bool, output: &mut impl Write) -> CliResult<()> {
    if !json {
        if let CliEvent::Output(bytes) = event {
            output.write_all(bytes)?;
            output.flush()?;
            return Ok(());
        }
    }
    let line = if json {
        json_event(event)
    } else {
        human_event(event)
    };
    writeln!(output, "{line}")?;
    Ok(())
}

fn human_event(event: &CliEvent) -> String {
    match event {
        CliEvent::Started(summary) => format!("STARTED {}", summary.name),
        CliEvent::Progress { phase, value } => {
            format!("PROGRESS {phase:?} {} {:?}", value.current, value.total)
        }
        CliEvent::Output(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        CliEvent::Diff(page) => format!("DIFF {:?}", page.entries),
        CliEvent::Snapshot(snapshot) => human_result(&CommandResult::Query(snapshot.clone())),
        CliEvent::Finished(Ok(result)) => format!("FINISHED {}", human_result(result)),
        CliEvent::Finished(Err(error)) => format!("FAILED {error}"),
    }
}

fn human_result(result: &CommandResult) -> String {
    match result {
        CommandResult::Empty => String::new(),
        CommandResult::Text(text) => text.clone(),
        CommandResult::Query(page) => format!("{:?}", page.items),
        CommandResult::Monitor(snapshot) => format!("{snapshot:?}"),
        CommandResult::Dedup(analysis) => format!("{analysis:?}"),
    }
}

fn json_event(event: &CliEvent) -> String {
    if let CliEvent::Finished(Ok(result)) = event {
        return format!(
            "{{\"schema_version\":3,\"event\":\"finished\",\"result\":{}}}",
            json_result(result)
        );
    }
    if let CliEvent::Diff(page) = event {
        return format!(
            "{{\"schema_version\":3,\"event\":\"diff\",\"page\":{}}}",
            json_diff_page(page)
        );
    }
    if let CliEvent::Snapshot(page) = event {
        return format!(
            "{{\"schema_version\":3,\"event\":\"snapshot\",\"result\":{}}}",
            json_result(&CommandResult::Query(page.clone()))
        );
    }
    let (kind, value) = match event {
        CliEvent::Started(summary) => ("started", summary.name.clone()),
        CliEvent::Progress { phase, value } => (
            "progress",
            format!("{phase:?}:{}:{:?}", value.current, value.total),
        ),
        CliEvent::Output(bytes) => ("output", String::from_utf8_lossy(bytes).into_owned()),
        CliEvent::Diff(_) | CliEvent::Snapshot(_) => unreachable!("handled above"),
        CliEvent::Finished(Ok(_)) => unreachable!("handled above"),
        CliEvent::Finished(Err(error)) => ("failed", error.to_string()),
    };
    format!(
        "{{\"schema_version\":3,\"event\":\"{kind}\",\"value\":\"{}\"}}",
        escape(&value)
    )
}

fn json_result(result: &CommandResult) -> String {
    match result {
        CommandResult::Empty => "{\"kind\":\"empty\"}".to_owned(),
        CommandResult::Text(value) => {
            format!("{{\"kind\":\"text\",\"value\":\"{}\"}}", escape(value))
        }
        CommandResult::Query(page) => format!(
            "{{\"kind\":\"query\",\"items\":[{}],\"continuation\":{}}}",
            page.items
                .iter()
                .map(json_query_item)
                .collect::<Vec<_>>()
                .join(","),
            page.continuation
                .as_deref()
                .map(|value| format!("\"{}\"", hex(value)))
                .unwrap_or_else(|| "null".to_owned())
        ),
        CommandResult::Monitor(snapshot) => monitor_json(snapshot),
        CommandResult::Dedup(analysis) => dedup_json(analysis),
    }
}

fn json_query_item(item: &layerfs_sdk::QueryItem) -> String {
    use layerfs_sdk::{BranchScope, Fact, QueryItem};
    match item {
        QueryItem::LayerStack(value) => format!(
            "{{\"type\":\"layer_stack\",\"id\":\"{}\",\"name\":\"{}\",\"head_layer_id\":\"{}\"}}",
            value.id,
            escape(value.name.as_str()),
            value.head_layer_id,
        ),
        QueryItem::LayerStackScope(fact, scope) => format!(
            "{{\"type\":\"layer_stack_scope\",\"id\":\"{}\",\"name\":\"{}\",\"through_layer_id\":\"{}\",\"serving_mode\":\"{}\"}}",
            fact.id,
            escape(fact.name.as_str()),
            scope.through_layer_id,
            placement(scope.serving_mode),
        ),
        QueryItem::Branch(value) => branch_json("branch", value, None),
        QueryItem::BranchScope(value, scope) => branch_json(
            "branch_scope",
            value,
            Some(match scope.scope {
                BranchScope::Local => "\"scope\":\"local\"".to_owned(),
                BranchScope::Remote {
                    through_commit_id,
                    serving_mode,
                } => format!(
                    "\"scope\":\"remote\",\"through_commit_id\":\"{through_commit_id}\",\"serving_mode\":\"{}\"",
                    placement(serving_mode)
                ),
            }),
        ),
        QueryItem::Fact(Fact::LayerStack(value)) => format!(
            "{{\"type\":\"layer_stack_fact\",\"id\":\"{}\",\"name\":\"{}\"}}",
            value.id,
            escape(value.name.as_str()),
        ),
        QueryItem::Fact(Fact::Layer(value)) => format!(
            "{{\"type\":\"layer_fact\",\"id\":\"{}\",\"layer_stack_id\":\"{}\",\"parent_layer_id\":{},\"root_id\":\"{}\"}}",
            value.id,
            value.layer_stack_id,
            optional_display(value.parent_layer_id),
            value.root_id,
        ),
        QueryItem::Fact(Fact::Branch(value)) => format!(
            "{{\"type\":\"branch_fact\",\"id\":\"{}\",\"layer_stack_id\":\"{}\",\"name\":\"{}\"}}",
            value.id,
            value.layer_stack_id,
            escape(value.name.as_str()),
        ),
        QueryItem::Fact(Fact::Commit(value)) => format!(
            "{{\"type\":\"commit_fact\",\"id\":\"{}\",\"root_id\":\"{}\",\"parent_commit_id\":{},\"base_layer_id\":\"{}\"}}",
            value.id,
            value.root_id,
            optional_display(value.parent_commit_id),
            value.base_layer_id,
        ),
        QueryItem::Workspace(value) => format!(
            "{{\"type\":\"workspace\",\"id\":\"{}\",\"layer_stack_id\":\"{}\",\"layer_stack_name\":\"{}\",\"branch_id\":\"{}\",\"branch_name\":\"{}\",\"pinned_head\":{},\"state\":\"{}\",\"dirty\":{}}}",
            value.summary.id,
            value.layer_stack_id,
            escape(value.layer_stack_name.as_str()),
            value.summary.branch_id,
            escape(value.branch_name.as_str()),
            optional_display(value.summary.pinned_head),
            format!("{:?}", value.summary.state).to_lowercase(),
            value.summary.dirty,
        ),
        QueryItem::Monitor(value) => monitor_json(value),
    }
}

fn branch_json(kind: &str, value: &layerfs_sdk::BranchRecord, scope: Option<String>) -> String {
    let mut json = format!(
        "{{\"type\":\"{kind}\",\"id\":\"{}\",\"layer_stack_id\":\"{}\",\"name\":\"{}\",\"base_layer_id\":\"{}\",\"head_commit_id\":{},\"forked_from_layer_id\":{},\"forked_from_branch_id\":{},\"forked_from_commit_id\":{}",
        value.id,
        value.layer_stack_id,
        escape(value.name.as_str()),
        value.base_layer_id,
        optional_display(value.head_commit_id),
        optional_display(value.forked_from_layer_id),
        optional_display(value.forked_from_branch_id),
        optional_display(value.forked_from_commit_id),
    );
    if let Some(scope) = scope {
        json.push(',');
        json.push_str(&scope);
    }
    json.push('}');
    json
}

fn json_diff_page(page: &layerfs_sdk::DiffPage) -> String {
    format!(
        "{{\"entries\":[{}],\"continuation\":{}}}",
        page.entries
            .iter()
            .map(json_diff_entry)
            .collect::<Vec<_>>()
            .join(","),
        page.continuation
            .map(|value| value.to_string())
            .unwrap_or_else(|| "null".to_owned())
    )
}

fn json_diff_entry(entry: &layerfs_sdk::DiffEntry) -> String {
    match entry {
        layerfs_sdk::DiffEntry::Add { path, after } => format!(
            "{{\"type\":\"add\",\"path\":\"{}\",\"after\":{}}}",
            escape(path.as_str()),
            node_json(*after)
        ),
        layerfs_sdk::DiffEntry::Remove { path, before } => format!(
            "{{\"type\":\"remove\",\"path\":\"{}\",\"before\":{}}}",
            escape(path.as_str()),
            node_json(*before)
        ),
        layerfs_sdk::DiffEntry::Modify {
            path,
            before,
            after,
            aspects,
        } => format!(
            "{{\"type\":\"modify\",\"path\":\"{}\",\"before\":{},\"after\":{},\"aspects\":{{\"node_type\":{},\"content\":{},\"metadata\":{},\"directory_membership\":{},\"hard_links\":{}}}}}",
            escape(path.as_str()),
            node_json(*before),
            node_json(*after),
            aspects.node_type,
            aspects.content,
            aspects.metadata,
            aspects.directory_membership,
            aspects.hard_links,
        ),
    }
}

fn node_json(node: layerfs_sdk::NodeSummary) -> String {
    format!(
        "{{\"kind\":\"{}\",\"content_root\":\"{}\",\"metadata_root\":\"{}\",\"namespace_ref_count\":{}}}",
        format!("{:?}", node.kind).to_lowercase(),
        node.content_root,
        node.metadata_root,
        node.namespace_ref_count,
    )
}

fn monitor_json(snapshot: &layerfs_sdk::MonitorSnapshot) -> String {
    format!(
        "{{\"kind\":\"monitor\",\"process_id\":{},\"resident_bytes\":{},\"available_parallelism\":{},\"databases\":[{}],\"workspaces\":[{}],\"operations\":[{}],\"dedup\":{}}}",
        snapshot.process_id,
        snapshot
            .resident_bytes
            .map(|value| value.to_string())
            .unwrap_or_else(|| "null".to_owned()),
        snapshot.available_parallelism,
        snapshot
            .databases
            .iter()
            .map(|database| format!(
                "{{\"role\":\"{}\",\"location\":\"{}\",\"database_bytes\":{},\"wal_bytes\":{},\"shm_bytes\":{}}}",
                escape(&database.role),
                escape(&database.location),
                database.storage.database_bytes,
                database.storage.wal_bytes,
                database.storage.shm_bytes,
            ))
            .collect::<Vec<_>>()
            .join(","),
        snapshot
            .workspaces
            .iter()
            .map(workspace_summary_json)
            .collect::<Vec<_>>()
            .join(","),
        snapshot
            .operations
            .iter()
            .map(layerfs_sdk::OperationReceipt::to_json)
            .collect::<Vec<_>>()
            .join(","),
        snapshot
            .dedup
            .as_ref()
            .map(dedup_json)
            .unwrap_or_else(|| "null".to_owned()),
    )
}

fn workspace_summary_json(value: &layerfs_sdk::WorkspaceSummary) -> String {
    format!(
        "{{\"id\":\"{}\",\"branch_id\":\"{}\",\"pinned_head\":{},\"state\":\"{}\",\"dirty\":{}}}",
        value.id,
        value.branch_id,
        optional_display(value.pinned_head),
        format!("{:?}", value.state).to_lowercase(),
        value.dirty,
    )
}

fn dedup_json(analysis: &layerfs_sdk::DedupAnalysis) -> String {
    let local = match &analysis.local_cas {
        layerfs_sdk::ExactOrUnavailable::Exact(value) => format!(
            "{{\"candidate_bytes\":{},\"inserted_bytes\":{},\"reused_bytes\":{},\"saved_fraction\":{},\"logical_to_physical\":{}}}",
            value.candidate_bytes,
            value.inserted_bytes,
            value.reused_bytes,
            json_float(value.saved_fraction),
            json_float(value.logical_to_physical),
        ),
        layerfs_sdk::ExactOrUnavailable::Unavailable(reason) => {
            format!("{{\"unavailable\":\"{}\"}}", escape(reason))
        }
    };
    let transfer = match &analysis.transfer {
        layerfs_sdk::ExactOrUnavailable::Exact(value) => format!(
            "{{\"announced_bytes\":{},\"sent_bytes\":{},\"avoided_bytes\":{},\"avoided_fraction\":{}}}",
            value.announced_bytes,
            value.sent_bytes,
            value.avoided_bytes,
            json_float(value.avoided_fraction),
        ),
        layerfs_sdk::ExactOrUnavailable::Unavailable(reason) => {
            format!("{{\"unavailable\":\"{}\"}}", escape(reason))
        }
    };
    format!(
        "{{\"kind\":\"dedup\",\"physical_cas_bytes\":{},\"union_cas_bytes\":{},\"cross_store_placement_bytes\":{},\"placement_factor\":{},\"placements\":[{}],\"local_cas\":{},\"transfer\":{}}}",
        analysis.physical_cas_bytes,
        analysis.union_cas_bytes,
        analysis.cross_store_placement_bytes,
        json_float(analysis.placement_factor),
        analysis
            .placements
            .iter()
            .map(|placement| format!(
                "{{\"role\":\"{}\",\"object_count\":{},\"encoded_bytes\":{}}}",
                escape(&placement.role),
                placement.object_count,
                placement.encoded_bytes,
            ))
            .collect::<Vec<_>>()
            .join(","),
        local,
        transfer,
    )
}

fn json_float(value: f64) -> String {
    if value.is_finite() {
        value.to_string()
    } else {
        "null".to_owned()
    }
}

fn placement(value: layerfs_sdk::RemotePlacement) -> &'static str {
    match value {
        layerfs_sdk::RemotePlacement::Reference => "reference",
        layerfs_sdk::RemotePlacement::Replica => "replica",
    }
}

fn optional_display(value: Option<impl std::fmt::Display>) -> String {
    value
        .map(|value| format!("\"{value}\""))
        .unwrap_or_else(|| "null".to_owned())
}

fn hex(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn escape(value: &str) -> String {
    let mut output = String::new();
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character <= '\u{1f}' => {
                output.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => output.push(character),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use layerfs_sdk::{
        EntityName, LayerId, LayerStackFact, LayerStackId, LayerStackScopeRecord, QueryItem,
        QueryPage, RemotePlacement,
    };

    #[test]
    fn query_json_names_the_scope_boundary_and_mode() {
        let stack = LayerStackId::new();
        let mut layer_bytes = [0; 33];
        layer_bytes[0] = 0x32;
        let through = LayerId::from_bytes(layer_bytes).unwrap();
        let event = CliEvent::Finished(Ok(CommandResult::Query(QueryPage {
            items: vec![QueryItem::LayerStackScope(
                LayerStackFact {
                    id: stack,
                    name: EntityName::new("api-server").unwrap(),
                },
                LayerStackScopeRecord {
                    layer_stack_id: stack,
                    through_layer_id: through,
                    serving_mode: RemotePlacement::Replica,
                },
            )],
            continuation: None,
        })));
        let json = json_event(&event);
        assert!(json.contains("\"schema_version\":3"));
        assert!(json.contains("\"name\":\"api-server\""));
        assert!(json.contains(&format!("\"through_layer_id\":\"{through}\"")));
        assert!(json.contains("\"serving_mode\":\"replica\""));
        assert!(!json.contains("LayerStackScope("));
    }

    #[test]
    fn json_output_escapes_every_control_character() {
        let json = json_event(&CliEvent::Output(b"tab\treturn\rzero\0line\n".to_vec()));
        assert!(json.contains("tab\\treturn\\rzero\\u0000line\\n"));
        assert!(!json.contains('\t'));
        assert!(!json.contains('\r'));
        assert!(!json.contains('\0'));
    }

    #[test]
    fn human_output_preserves_bytes_without_line_framing() {
        let mut output = Vec::new();
        render(
            &CliEvent::Output(b"first\nsecond\0".to_vec()),
            false,
            &mut output,
        )
        .unwrap();
        assert_eq!(output, b"first\nsecond\0");
    }
}

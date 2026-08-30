use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use layerfs_storage::{BranchId, CommitId, EntityName, LayerId, LayerStackId, RemotePlacement};
use layerfs_workspace::{ExecutionId, WorkspaceId};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OperationId([u8; 16]);

impl OperationId {
    pub fn new() -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&std::process::id().to_be_bytes());
        bytes.extend_from_slice(
            &std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
                .to_be_bytes(),
        );
        bytes.extend_from_slice(&SERIAL.fetch_add(1, Ordering::Relaxed).to_be_bytes());
        let digest = layerfs_content::ObjectId::for_bytes(&bytes).to_bytes();
        Self(digest[..16].try_into().expect("fixed operation id"))
    }
}

impl Default for OperationId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for OperationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("o:")?;
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl std::str::FromStr for OperationId {
    type Err = crate::MonitorError;

    fn from_str(value: &str) -> crate::MonitorResult<Self> {
        let value = value
            .strip_prefix("o:")
            .ok_or(crate::MonitorError::NotFound)?;
        if value.len() != 32 {
            return Err(crate::MonitorError::NotFound);
        }
        let mut bytes = [0; 16];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            bytes[index] = (hex(pair[0])? << 4) | hex(pair[1])?;
        }
        Ok(Self(bytes))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationOutcome {
    Succeeded,
    Failed,
    Interrupted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimingFragment {
    pub process_id: u32,
    pub started_ns: u64,
    pub elapsed_ns: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationReceipt {
    pub id: OperationId,
    pub operation: SemanticOperation,
    pub outcome: OperationOutcome,
    pub queued_ns: u64,
    pub service_ns: u64,
    pub fragments: Vec<TimingFragment>,
    pub storage: Vec<layerfs_storage::StorageReceipt>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationFamily {
    LayerStackInitialize,
    LayerStackPull,
    LayerStackDiff,
    LayerStackAdd,
    BranchPull,
    BranchFork,
    BranchDiff,
    BranchPush,
    WorkspaceCreate,
    WorkspaceExec,
    WorkspaceShell,
    WorkspaceOutput,
    WorkspaceStop,
    WorkspaceConflicts,
    WorkspaceResolve,
    WorkspaceCommit,
    WorkspaceEnd,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticOperation {
    pub family: OperationFamily,
    pub layer_stack_id: Option<LayerStackId>,
    pub layer_stack_name: Option<EntityName>,
    pub branch_id: Option<BranchId>,
    pub branch_name: Option<EntityName>,
    pub through_layer_id: Option<LayerId>,
    pub through_commit_id: Option<CommitId>,
    pub placement: Option<RemotePlacement>,
    pub workspace_id: Option<WorkspaceId>,
    pub execution_id: Option<ExecutionId>,
    pub result_layer_id: Option<LayerId>,
    pub result_commit_id: Option<CommitId>,
}

impl SemanticOperation {
    pub fn new(family: OperationFamily) -> Self {
        Self {
            family,
            layer_stack_id: None,
            layer_stack_name: None,
            branch_id: None,
            branch_name: None,
            through_layer_id: None,
            through_commit_id: None,
            placement: None,
            workspace_id: None,
            execution_id: None,
            result_layer_id: None,
            result_commit_id: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CliInvocationReceipt {
    pub operation_id: OperationId,
    pub outcome: OperationOutcome,
    pub total_elapsed_ns: u64,
    pub parse_elapsed_ns: u64,
    pub context_open_elapsed_ns: u64,
    pub operation_wait_elapsed_ns: u64,
    pub render_elapsed_ns: u64,
}

impl CliInvocationReceipt {
    pub fn timing_is_consistent(&self) -> bool {
        self.total_elapsed_ns
            >= self
                .parse_elapsed_ns
                .saturating_add(self.context_open_elapsed_ns)
                .saturating_add(self.operation_wait_elapsed_ns)
                .saturating_add(self.render_elapsed_ns)
    }
}

impl OperationReceipt {
    pub fn timing_is_consistent(&self) -> bool {
        self.fragments.iter().all(|fragment| {
            fragment
                .started_ns
                .checked_add(fragment.elapsed_ns)
                .is_some()
        }) && self
            .fragments
            .iter()
            .map(|fragment| fragment.elapsed_ns)
            .sum::<u64>()
            <= self.service_ns.saturating_mul(64)
            && self.storage.iter().all(|receipt| match receipt {
                layerfs_storage::StorageReceipt::Local(receipt) => receipt.validate().is_ok(),
                layerfs_storage::StorageReceipt::Transfer(receipt) => receipt.validate().is_ok(),
            })
    }

    pub fn to_json(&self) -> String {
        format!(
            "{{\"schema\":\"layerfs-operation-receipt-v3\",\"record\":\"{}\",\"operation_id\":\"{}\",\"operation\":{},\"outcome\":\"{}\",\"queued_ns\":{},\"service_ns\":{},\"fragments\":[{}],\"storage\":[{}]}}",
            hex_bytes(self.record().as_bytes()),
            self.id,
            operation_json(&self.operation),
            format!("{:?}", self.outcome).to_lowercase(),
            self.queued_ns,
            self.service_ns,
            self.fragments
                .iter()
                .map(|fragment| format!(
                    "{{\"process_id\":{},\"started_ns\":{},\"elapsed_ns\":{}}}",
                    fragment.process_id, fragment.started_ns, fragment.elapsed_ns
                ))
                .collect::<Vec<_>>()
                .join(","),
            self.storage.iter().map(storage_json).collect::<Vec<_>>().join(","),
        )
    }

    pub(crate) fn from_json(line: &str) -> crate::MonitorResult<Self> {
        let encoded = line
            .split_once("\"record\":\"")
            .and_then(|(_, tail)| tail.split_once('"').map(|(record, _)| record))
            .ok_or(crate::MonitorError::Integrity("operation record"))?;
        let bytes = decode_hex(encoded)?;
        let record = std::str::from_utf8(&bytes)
            .map_err(|_| crate::MonitorError::Integrity("operation record UTF-8"))?;
        Self::from_record(record)
    }

    fn record(&self) -> String {
        let mut fields = vec![
            "v3".to_owned(),
            self.id.to_string(),
            family_code(self.operation.family).to_owned(),
            optional_id(self.operation.layer_stack_id),
            optional_name(self.operation.layer_stack_name.as_ref()),
            optional_id(self.operation.branch_id),
            optional_name(self.operation.branch_name.as_ref()),
            optional_id(self.operation.through_layer_id),
            optional_id(self.operation.through_commit_id),
            placement_code(self.operation.placement).to_owned(),
            optional_id(self.operation.workspace_id),
            optional_id(self.operation.execution_id),
            optional_id(self.operation.result_layer_id),
            optional_id(self.operation.result_commit_id),
            match self.outcome {
                OperationOutcome::Succeeded => "s",
                OperationOutcome::Failed => "f",
                OperationOutcome::Interrupted => "i",
            }
            .to_owned(),
            self.queued_ns.to_string(),
            self.service_ns.to_string(),
            self.fragments.len().to_string(),
        ];
        for fragment in &self.fragments {
            fields.extend([
                fragment.process_id.to_string(),
                fragment.started_ns.to_string(),
                fragment.elapsed_ns.to_string(),
            ]);
        }
        fields.push(self.storage.len().to_string());
        for receipt in &self.storage {
            match receipt {
                layerfs_storage::StorageReceipt::Local(receipt) => {
                    let value = receipt.objects;
                    fields.push("l".to_owned());
                    fields.extend(
                        [
                            value.candidate_ids,
                            value.candidate_bytes,
                            value.inserted_ids,
                            value.inserted_bytes,
                            value.reused_ids,
                            value.reused_bytes,
                        ]
                        .map(|value| value.to_string()),
                    );
                }
                layerfs_storage::StorageReceipt::Transfer(receipt) => {
                    fields.push("t".to_owned());
                    fields.extend(
                        [
                            receipt.membership_pages,
                            receipt.payload_batches,
                            receipt.peak_buffer_bytes,
                            receipt.known_roots_pruned,
                        ]
                        .map(|value| value.to_string()),
                    );
                    push_set(&mut fields, receipt.objects);
                    fields.push(receipt.facts.len().to_string());
                    for (kind, set) in &receipt.facts {
                        fields.push(fact_kind(*kind).to_string());
                        push_set(&mut fields, *set);
                    }
                }
            }
        }
        fields.join(" ")
    }

    fn from_record(record: &str) -> crate::MonitorResult<Self> {
        let mut fields = record.split_whitespace();
        expect(&mut fields, "v3")?;
        let id = next(&mut fields)?.parse()?;
        let operation = SemanticOperation {
            family: take_family(&mut fields)?,
            layer_stack_id: take_optional_id(&mut fields)?,
            layer_stack_name: take_optional_name(&mut fields)?,
            branch_id: take_optional_id(&mut fields)?,
            branch_name: take_optional_name(&mut fields)?,
            through_layer_id: take_optional_id(&mut fields)?,
            through_commit_id: take_optional_id(&mut fields)?,
            placement: take_placement(&mut fields)?,
            workspace_id: take_optional_id(&mut fields)?,
            execution_id: take_optional_id(&mut fields)?,
            result_layer_id: take_optional_id(&mut fields)?,
            result_commit_id: take_optional_id(&mut fields)?,
        };
        let outcome = match next(&mut fields)? {
            "s" => OperationOutcome::Succeeded,
            "f" => OperationOutcome::Failed,
            "i" => OperationOutcome::Interrupted,
            _ => return Err(crate::MonitorError::Integrity("operation outcome")),
        };
        let queued_ns = number(&mut fields)?;
        let service_ns = number(&mut fields)?;
        let mut fragments = Vec::new();
        for _ in 0..count(&mut fields)? {
            fragments.push(TimingFragment {
                process_id: number(&mut fields)?,
                started_ns: number(&mut fields)?,
                elapsed_ns: number(&mut fields)?,
            });
        }
        let mut storage = Vec::new();
        for _ in 0..count(&mut fields)? {
            storage.push(match next(&mut fields)? {
                "l" => {
                    layerfs_storage::StorageReceipt::Local(layerfs_storage::LocalAdmissionReceipt {
                        objects: layerfs_storage::LocalObjectReceipt {
                            candidate_ids: number(&mut fields)?,
                            candidate_bytes: number(&mut fields)?,
                            inserted_ids: number(&mut fields)?,
                            inserted_bytes: number(&mut fields)?,
                            reused_ids: number(&mut fields)?,
                            reused_bytes: number(&mut fields)?,
                        },
                    })
                }
                "t" => {
                    let membership_pages = number(&mut fields)?;
                    let payload_batches = number(&mut fields)?;
                    let peak_buffer_bytes = number(&mut fields)?;
                    let known_roots_pruned = number(&mut fields)?;
                    let objects = take_set(&mut fields)?;
                    let mut facts = std::collections::BTreeMap::new();
                    for _ in 0..count(&mut fields)? {
                        facts.insert(take_fact_kind(&mut fields)?, take_set(&mut fields)?);
                    }
                    layerfs_storage::StorageReceipt::Transfer(layerfs_storage::TransferReceipt {
                        objects,
                        facts,
                        membership_pages,
                        payload_batches,
                        peak_buffer_bytes,
                        known_roots_pruned,
                    })
                }
                _ => return Err(crate::MonitorError::Integrity("storage receipt kind")),
            });
        }
        if fields.next().is_some() {
            return Err(crate::MonitorError::Integrity(
                "operation record trailing fields",
            ));
        }
        let receipt = Self {
            id,
            operation,
            outcome,
            queued_ns,
            service_ns,
            fragments,
            storage,
        };
        if !receipt.timing_is_consistent() {
            return Err(crate::MonitorError::Integrity("operation record equation"));
        }
        Ok(receipt)
    }
}

fn operation_json(operation: &SemanticOperation) -> String {
    format!(
        "{{\"family\":\"{}\",\"layer_stack_id\":{},\"layer_stack_name\":{},\"branch_id\":{},\"branch_name\":{},\"through_layer_id\":{},\"through_commit_id\":{},\"placement\":{},\"workspace_id\":{},\"execution_id\":{},\"result_layer_id\":{},\"result_commit_id\":{}}}",
        family_name(operation.family),
        json_optional(operation.layer_stack_id),
        json_optional_name(operation.layer_stack_name.as_ref()),
        json_optional(operation.branch_id),
        json_optional_name(operation.branch_name.as_ref()),
        json_optional(operation.through_layer_id),
        json_optional(operation.through_commit_id),
        operation
            .placement
            .map(|placement| format!("\"{}\"", placement_name(placement)))
            .unwrap_or_else(|| "null".to_owned()),
        json_optional(operation.workspace_id),
        json_optional(operation.execution_id),
        json_optional(operation.result_layer_id),
        json_optional(operation.result_commit_id),
    )
}

fn json_optional(value: Option<impl fmt::Display>) -> String {
    value
        .map(|value| format!("\"{value}\""))
        .unwrap_or_else(|| "null".to_owned())
}

fn json_optional_name(value: Option<&EntityName>) -> String {
    value
        .map(|value| format!("\"{}\"", escape(value.as_str())))
        .unwrap_or_else(|| "null".to_owned())
}

fn family_name(family: OperationFamily) -> &'static str {
    match family {
        OperationFamily::LayerStackInitialize => "layerstack.initialize",
        OperationFamily::LayerStackPull => "layerstack.pull",
        OperationFamily::LayerStackDiff => "layerstack.diff",
        OperationFamily::LayerStackAdd => "layerstack.add",
        OperationFamily::BranchPull => "branch.pull",
        OperationFamily::BranchFork => "branch.fork",
        OperationFamily::BranchDiff => "branch.diff",
        OperationFamily::BranchPush => "branch.push",
        OperationFamily::WorkspaceCreate => "workspace.create",
        OperationFamily::WorkspaceExec => "workspace.exec",
        OperationFamily::WorkspaceShell => "workspace.shell",
        OperationFamily::WorkspaceOutput => "workspace.output",
        OperationFamily::WorkspaceStop => "workspace.stop",
        OperationFamily::WorkspaceConflicts => "workspace.conflicts",
        OperationFamily::WorkspaceResolve => "workspace.resolve",
        OperationFamily::WorkspaceCommit => "workspace.commit",
        OperationFamily::WorkspaceEnd => "workspace.end",
    }
}

fn family_code(family: OperationFamily) -> &'static str {
    match family {
        OperationFamily::LayerStackInitialize => "li",
        OperationFamily::LayerStackPull => "lp",
        OperationFamily::LayerStackDiff => "ld",
        OperationFamily::LayerStackAdd => "la",
        OperationFamily::BranchPull => "bp",
        OperationFamily::BranchFork => "bf",
        OperationFamily::BranchDiff => "bd",
        OperationFamily::BranchPush => "bu",
        OperationFamily::WorkspaceCreate => "wc",
        OperationFamily::WorkspaceExec => "wx",
        OperationFamily::WorkspaceShell => "wh",
        OperationFamily::WorkspaceOutput => "wo",
        OperationFamily::WorkspaceStop => "ws",
        OperationFamily::WorkspaceConflicts => "wf",
        OperationFamily::WorkspaceResolve => "wr",
        OperationFamily::WorkspaceCommit => "wm",
        OperationFamily::WorkspaceEnd => "we",
    }
}

fn take_family<'a>(
    fields: &mut impl Iterator<Item = &'a str>,
) -> crate::MonitorResult<OperationFamily> {
    match next(fields)? {
        "li" => Ok(OperationFamily::LayerStackInitialize),
        "lp" => Ok(OperationFamily::LayerStackPull),
        "ld" => Ok(OperationFamily::LayerStackDiff),
        "la" => Ok(OperationFamily::LayerStackAdd),
        "bp" => Ok(OperationFamily::BranchPull),
        "bf" => Ok(OperationFamily::BranchFork),
        "bd" => Ok(OperationFamily::BranchDiff),
        "bu" => Ok(OperationFamily::BranchPush),
        "wc" => Ok(OperationFamily::WorkspaceCreate),
        "wx" => Ok(OperationFamily::WorkspaceExec),
        "wh" => Ok(OperationFamily::WorkspaceShell),
        "wo" => Ok(OperationFamily::WorkspaceOutput),
        "ws" => Ok(OperationFamily::WorkspaceStop),
        "wf" => Ok(OperationFamily::WorkspaceConflicts),
        "wr" => Ok(OperationFamily::WorkspaceResolve),
        "wm" => Ok(OperationFamily::WorkspaceCommit),
        "we" => Ok(OperationFamily::WorkspaceEnd),
        _ => Err(crate::MonitorError::Integrity("operation family")),
    }
}

fn optional_id(value: Option<impl fmt::Display>) -> String {
    value.map_or_else(|| "-".to_owned(), |value| value.to_string())
}

fn optional_name(value: Option<&EntityName>) -> String {
    value.map_or_else(
        || "-".to_owned(),
        |value| hex_bytes(value.as_str().as_bytes()),
    )
}

fn take_optional_id<'a, T: std::str::FromStr>(
    fields: &mut impl Iterator<Item = &'a str>,
) -> crate::MonitorResult<Option<T>> {
    match next(fields)? {
        "-" => Ok(None),
        value => value
            .parse()
            .map(Some)
            .map_err(|_| crate::MonitorError::Integrity("operation identifier")),
    }
}

fn take_optional_name<'a>(
    fields: &mut impl Iterator<Item = &'a str>,
) -> crate::MonitorResult<Option<EntityName>> {
    match next(fields)? {
        "-" => Ok(None),
        value => String::from_utf8(decode_hex(value)?)
            .map_err(|_| crate::MonitorError::Integrity("operation name"))?
            .parse()
            .map(Some)
            .map_err(|_| crate::MonitorError::Integrity("operation name")),
    }
}

fn placement_name(placement: RemotePlacement) -> &'static str {
    match placement {
        RemotePlacement::Reference => "reference",
        RemotePlacement::Replica => "replica",
    }
}

fn placement_code(placement: Option<RemotePlacement>) -> &'static str {
    match placement {
        Some(RemotePlacement::Reference) => "r",
        Some(RemotePlacement::Replica) => "p",
        None => "-",
    }
}

fn take_placement<'a>(
    fields: &mut impl Iterator<Item = &'a str>,
) -> crate::MonitorResult<Option<RemotePlacement>> {
    match next(fields)? {
        "r" => Ok(Some(RemotePlacement::Reference)),
        "p" => Ok(Some(RemotePlacement::Replica)),
        "-" => Ok(None),
        _ => Err(crate::MonitorError::Integrity("operation placement")),
    }
}

fn storage_json(receipt: &layerfs_storage::StorageReceipt) -> String {
    match receipt {
        layerfs_storage::StorageReceipt::Local(receipt) => {
            let value = receipt.objects;
            format!(
                "{{\"local\":{{\"candidate_ids\":{},\"candidate_bytes\":{},\"inserted_ids\":{},\"inserted_bytes\":{},\"reused_ids\":{},\"reused_bytes\":{}}}}}",
                value.candidate_ids,
                value.candidate_bytes,
                value.inserted_ids,
                value.inserted_bytes,
                value.reused_ids,
                value.reused_bytes,
            )
        }
        layerfs_storage::StorageReceipt::Transfer(receipt) => format!(
            "{{\"transfer\":{{\"objects\":{},\"facts\":{{{}}},\"membership_pages\":{},\"payload_batches\":{},\"peak_buffer_bytes\":{},\"known_roots_pruned\":{}}}}}",
            set_json(receipt.objects),
            receipt
                .facts
                .iter()
                .map(|(kind, set)| format!("\"{}\":{}", fact_name(*kind), set_json(*set)))
                .collect::<Vec<_>>()
                .join(","),
            receipt.membership_pages,
            receipt.payload_batches,
            receipt.peak_buffer_bytes,
            receipt.known_roots_pruned,
        ),
    }
}

fn set_json(set: layerfs_storage::TransferSetReceipt) -> String {
    format!(
        "{{\"announced_ids\":{},\"announced_bytes\":{},\"missing_ids\":{},\"missing_bytes\":{},\"sent_ids\":{},\"sent_bytes\":{},\"inserted_ids\":{},\"inserted_bytes\":{},\"raced_existing_ids\":{},\"raced_existing_bytes\":{}}}",
        set.announced_ids,
        measured_json(set.announced_bytes),
        set.missing_ids,
        set.missing_bytes,
        set.sent_ids,
        set.sent_bytes,
        set.inserted_ids,
        set.inserted_bytes,
        set.raced_existing_ids,
        set.raced_existing_bytes,
    )
}

fn measured_json(value: layerfs_storage::MeasuredBytes) -> String {
    value
        .exact()
        .map_or_else(|| "null".to_owned(), |value| value.to_string())
}

fn push_set(fields: &mut Vec<String>, set: layerfs_storage::TransferSetReceipt) {
    fields.extend([
        set.announced_ids.to_string(),
        set.announced_bytes
            .exact()
            .map_or_else(|| "x".to_owned(), |bytes| bytes.to_string()),
        set.missing_ids.to_string(),
        set.missing_bytes.to_string(),
        set.sent_ids.to_string(),
        set.sent_bytes.to_string(),
        set.inserted_ids.to_string(),
        set.inserted_bytes.to_string(),
        set.raced_existing_ids.to_string(),
        set.raced_existing_bytes.to_string(),
    ]);
}

fn take_set<'a>(
    fields: &mut impl Iterator<Item = &'a str>,
) -> crate::MonitorResult<layerfs_storage::TransferSetReceipt> {
    let announced_ids = number(fields)?;
    let announced_bytes = match next(fields)? {
        "x" => layerfs_storage::MeasuredBytes::Unavailable,
        value => layerfs_storage::MeasuredBytes::Exact(parse(value)?),
    };
    Ok(layerfs_storage::TransferSetReceipt {
        announced_ids,
        announced_bytes,
        missing_ids: number(fields)?,
        missing_bytes: number(fields)?,
        sent_ids: number(fields)?,
        sent_bytes: number(fields)?,
        inserted_ids: number(fields)?,
        inserted_bytes: number(fields)?,
        raced_existing_ids: number(fields)?,
        raced_existing_bytes: number(fields)?,
    })
}

fn fact_kind(kind: layerfs_storage::FactKind) -> u8 {
    match kind {
        layerfs_storage::FactKind::Commit => 0,
        layerfs_storage::FactKind::Branch => 1,
        layerfs_storage::FactKind::LayerStack => 2,
        layerfs_storage::FactKind::Layer => 3,
    }
}

fn fact_name(kind: layerfs_storage::FactKind) -> &'static str {
    match kind {
        layerfs_storage::FactKind::Commit => "commit",
        layerfs_storage::FactKind::Branch => "branch",
        layerfs_storage::FactKind::LayerStack => "layerstack",
        layerfs_storage::FactKind::Layer => "layer",
    }
}

fn take_fact_kind<'a>(
    fields: &mut impl Iterator<Item = &'a str>,
) -> crate::MonitorResult<layerfs_storage::FactKind> {
    match number::<u8>(fields)? {
        0 => Ok(layerfs_storage::FactKind::Commit),
        1 => Ok(layerfs_storage::FactKind::Branch),
        2 => Ok(layerfs_storage::FactKind::LayerStack),
        3 => Ok(layerfs_storage::FactKind::Layer),
        _ => Err(crate::MonitorError::Integrity("fact kind")),
    }
}

fn expect<'a>(
    fields: &mut impl Iterator<Item = &'a str>,
    expected: &str,
) -> crate::MonitorResult<()> {
    if next(fields)? == expected {
        Ok(())
    } else {
        Err(crate::MonitorError::Integrity("operation record version"))
    }
}

fn next<'a>(fields: &mut impl Iterator<Item = &'a str>) -> crate::MonitorResult<&'a str> {
    fields
        .next()
        .ok_or(crate::MonitorError::Integrity("operation record field"))
}

fn count<'a>(fields: &mut impl Iterator<Item = &'a str>) -> crate::MonitorResult<usize> {
    number(fields)
}

fn number<'a, T: std::str::FromStr>(
    fields: &mut impl Iterator<Item = &'a str>,
) -> crate::MonitorResult<T> {
    parse(next(fields)?)
}

fn parse<T: std::str::FromStr>(value: &str) -> crate::MonitorResult<T> {
    value
        .parse()
        .map_err(|_| crate::MonitorError::Integrity("operation record number"))
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_hex(value: &str) -> crate::MonitorResult<Vec<u8>> {
    if value.len() % 2 != 0 {
        return Err(crate::MonitorError::Integrity("operation record hex"));
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| Ok((hex(pair[0])? << 4) | hex(pair[1])?))
        .collect()
}

fn hex(value: u8) -> crate::MonitorResult<u8> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(crate::MonitorError::NotFound),
    }
}

fn escape(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| match character {
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect(),
            character if character.is_control() => "?".chars().collect(),
            character => vec![character],
        })
        .collect()
}

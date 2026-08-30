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
                layerfs_storage::StorageReceipt::Durability(receipt) => receipt.validate().is_ok(),
                layerfs_storage::StorageReceipt::WorkspaceCommit(receipt) => {
                    receipt.validate().is_ok()
                }
                layerfs_storage::StorageReceipt::Push(receipt) => receipt.validate().is_ok(),
                layerfs_storage::StorageReceipt::Database(receipt) => receipt.validate().is_ok(),
                layerfs_storage::StorageReceipt::WorkspaceLifecycle(receipt) => {
                    receipt.validate().is_ok()
                }
            })
    }

    pub fn to_json(&self) -> String {
        format!(
            "{{\"schema\":\"layerfs-operation-receipt-v4\",\"record\":\"{}\",\"operation_id\":\"{}\",\"operation\":{},\"outcome\":\"{}\",\"queued_ns\":{},\"service_ns\":{},\"fragments\":[{}],\"storage\":[{}]}}",
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
            "v4".to_owned(),
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
                    fields.push("c".to_owned());
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
                    fields.extend([
                        receipt.cdc_bytes_scanned.to_string(),
                        receipt.encode_hash_invocations.to_string(),
                        receipt.source_reused_ids.to_string(),
                        receipt.source_reused_bytes.to_string(),
                    ]);
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
                layerfs_storage::StorageReceipt::Durability(receipt) => {
                    fields.push("d".to_owned());
                    fields.push(match receipt.role {
                        layerfs_storage::StoreRole::LayerStack => "s".to_owned(),
                        layerfs_storage::StoreRole::Branch => "b".to_owned(),
                    });
                    fields.push(receipt.store_id.to_string());
                    fields.extend(
                        [
                            receipt.stable_ns,
                            receipt.checkpoint_ns,
                            receipt.database_fsync_ns,
                            receipt.directory_fsync_ns,
                            receipt.unattributed_ns,
                        ]
                        .map(|value| value.to_string()),
                    );
                }
                layerfs_storage::StorageReceipt::WorkspaceCommit(receipt) => {
                    fields.push("w".to_owned());
                    fields.push(
                        match receipt.capture_mode {
                            Some(layerfs_storage::CaptureMode::Live) => "l",
                            Some(layerfs_storage::CaptureMode::Materialized) => "m",
                            None => "-",
                        }
                        .to_owned(),
                    );
                    fields.extend(
                        [
                            receipt.total_ns,
                            receipt.pause_fence_ns,
                            receipt.quiesce_ns,
                            receipt.capture_ns,
                            receipt.captured_files,
                            receipt.captured_bytes,
                            receipt.candidate_plan_ns,
                            receipt.dirty_compare_ns,
                            receipt.content_ns,
                            receipt.namespace_ns,
                            receipt.candidate_finish_ns,
                            receipt.local_admission_ns,
                            receipt.completeness_verify_ns,
                            receipt.publication_ns,
                            receipt.in_place_rebase_ns,
                            receipt.resume_ns,
                            receipt.unattributed_ns,
                        ]
                        .map(|value| value.to_string()),
                    );
                }
                layerfs_storage::StorageReceipt::Push(receipt) => {
                    fields.push("p".to_owned());
                    fields.extend(
                        [
                            receipt.total_ns,
                            receipt.history_ns,
                            receipt.frontier_ns,
                            receipt.membership_ns,
                            receipt.source_read_auth_ns,
                            receipt.object_admission_ns,
                            receipt.fact_admission_ns,
                            receipt.authority_transition_verify_ns,
                            receipt.publication_ns,
                            receipt.durability_ns,
                            receipt.unattributed_ns,
                            receipt.endpoint_calls,
                        ]
                        .map(|value| value.to_string()),
                    );
                }
                layerfs_storage::StorageReceipt::Database(receipt) => {
                    fields.push("q".to_owned());
                    fields.push(store_role_code(receipt.role).to_owned());
                    fields.push(receipt.store_id.to_string());
                    fields.push(database_operation_code(receipt.operation).to_owned());
                    fields.extend(
                        [
                            receipt.total_ns,
                            receipt.connection_wait_ns,
                            receipt.writer_acquire_ns,
                            receipt.statement_ns,
                            receipt.publication_ns,
                            receipt.commit_sync_ns,
                            receipt.unattributed_ns,
                            receipt.statement_count,
                            receipt.rows,
                            receipt.bytes,
                        ]
                        .map(|value| value.to_string()),
                    );
                }
                layerfs_storage::StorageReceipt::WorkspaceLifecycle(receipt) => {
                    fields.push("y".to_owned());
                    fields.push(
                        match receipt.kind {
                            layerfs_storage::WorkspaceLifecycleKind::Attach => "a",
                            layerfs_storage::WorkspaceLifecycleKind::End => "e",
                        }
                        .to_owned(),
                    );
                    fields.extend(
                        [
                            receipt.total_ns,
                            receipt.proxy_ns,
                            receipt.docker_setup_ns,
                            receipt.helper_copy_ns,
                            receipt.mount_ready_ns,
                            receipt.unmount_ns,
                            receipt.wait_ns,
                            receipt.cleanup_ns,
                            receipt.unattributed_ns,
                            receipt.docker_calls,
                        ]
                        .map(|value| value.to_string()),
                    );
                }
            }
        }
        fields.join(" ")
    }

    fn from_record(record: &str) -> crate::MonitorResult<Self> {
        let mut fields = record.split_whitespace();
        match next(&mut fields)? {
            "v3" | "v4" => {}
            _ => return Err(crate::MonitorError::Integrity("operation record version")),
        }
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
                kind @ ("l" | "b" | "c") => {
                    let objects = layerfs_storage::LocalObjectReceipt {
                        candidate_ids: number(&mut fields)?,
                        candidate_bytes: number(&mut fields)?,
                        inserted_ids: number(&mut fields)?,
                        inserted_bytes: number(&mut fields)?,
                        reused_ids: number(&mut fields)?,
                        reused_bytes: number(&mut fields)?,
                    };
                    layerfs_storage::StorageReceipt::Local(layerfs_storage::LocalAdmissionReceipt {
                        objects,
                        cdc_bytes_scanned: if matches!(kind, "b" | "c") {
                            number(&mut fields)?
                        } else {
                            0
                        },
                        encode_hash_invocations: if matches!(kind, "b" | "c") {
                            number(&mut fields)?
                        } else {
                            0
                        },
                        source_reused_ids: if kind == "c" { number(&mut fields)? } else { 0 },
                        source_reused_bytes: if kind == "c" { number(&mut fields)? } else { 0 },
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
                "d" => {
                    let role = match next(&mut fields)? {
                        "s" => layerfs_storage::StoreRole::LayerStack,
                        "b" => layerfs_storage::StoreRole::Branch,
                        _ => return Err(crate::MonitorError::Integrity("durability Store role")),
                    };
                    layerfs_storage::StorageReceipt::Durability(
                        layerfs_storage::DurabilityReceipt {
                            store_id: next(&mut fields)?
                                .parse()
                                .map_err(|_| crate::MonitorError::Integrity("durability Store"))?,
                            role,
                            stable_ns: number(&mut fields)?,
                            checkpoint_ns: number(&mut fields)?,
                            database_fsync_ns: number(&mut fields)?,
                            directory_fsync_ns: number(&mut fields)?,
                            unattributed_ns: number(&mut fields)?,
                        },
                    )
                }
                "w" => layerfs_storage::StorageReceipt::WorkspaceCommit(
                    layerfs_storage::WorkspaceCommitReceipt {
                        capture_mode: match next(&mut fields)? {
                            "l" => Some(layerfs_storage::CaptureMode::Live),
                            "m" => Some(layerfs_storage::CaptureMode::Materialized),
                            "-" => None,
                            _ => return Err(crate::MonitorError::Integrity("capture mode")),
                        },
                        total_ns: number(&mut fields)?,
                        pause_fence_ns: number(&mut fields)?,
                        quiesce_ns: number(&mut fields)?,
                        capture_ns: number(&mut fields)?,
                        captured_files: number(&mut fields)?,
                        captured_bytes: number(&mut fields)?,
                        candidate_plan_ns: number(&mut fields)?,
                        dirty_compare_ns: number(&mut fields)?,
                        content_ns: number(&mut fields)?,
                        namespace_ns: number(&mut fields)?,
                        candidate_finish_ns: number(&mut fields)?,
                        local_admission_ns: number(&mut fields)?,
                        completeness_verify_ns: number(&mut fields)?,
                        publication_ns: number(&mut fields)?,
                        in_place_rebase_ns: number(&mut fields)?,
                        resume_ns: number(&mut fields)?,
                        unattributed_ns: number(&mut fields)?,
                    },
                ),
                "p" => layerfs_storage::StorageReceipt::Push(layerfs_storage::PushPhaseReceipt {
                    total_ns: number(&mut fields)?,
                    history_ns: number(&mut fields)?,
                    frontier_ns: number(&mut fields)?,
                    membership_ns: number(&mut fields)?,
                    source_read_auth_ns: number(&mut fields)?,
                    object_admission_ns: number(&mut fields)?,
                    fact_admission_ns: number(&mut fields)?,
                    authority_transition_verify_ns: number(&mut fields)?,
                    publication_ns: number(&mut fields)?,
                    durability_ns: number(&mut fields)?,
                    unattributed_ns: number(&mut fields)?,
                    endpoint_calls: number(&mut fields)?,
                }),
                "q" => {
                    layerfs_storage::StorageReceipt::Database(layerfs_storage::DatabaseReceipt {
                        role: take_store_role(&mut fields)?,
                        store_id: next(&mut fields)?
                            .parse()
                            .map_err(|_| crate::MonitorError::Integrity("database Store"))?,
                        operation: take_database_operation(&mut fields)?,
                        total_ns: number(&mut fields)?,
                        connection_wait_ns: number(&mut fields)?,
                        writer_acquire_ns: number(&mut fields)?,
                        statement_ns: number(&mut fields)?,
                        publication_ns: number(&mut fields)?,
                        commit_sync_ns: number(&mut fields)?,
                        unattributed_ns: number(&mut fields)?,
                        statement_count: number(&mut fields)?,
                        rows: number(&mut fields)?,
                        bytes: number(&mut fields)?,
                    })
                }
                "y" => layerfs_storage::StorageReceipt::WorkspaceLifecycle(
                    layerfs_storage::WorkspaceLifecycleReceipt {
                        kind: match next(&mut fields)? {
                            "a" => layerfs_storage::WorkspaceLifecycleKind::Attach,
                            "e" => layerfs_storage::WorkspaceLifecycleKind::End,
                            _ => {
                                return Err(crate::MonitorError::Integrity(
                                    "Workspace lifecycle kind",
                                ))
                            }
                        },
                        total_ns: number(&mut fields)?,
                        proxy_ns: number(&mut fields)?,
                        docker_setup_ns: number(&mut fields)?,
                        helper_copy_ns: number(&mut fields)?,
                        mount_ready_ns: number(&mut fields)?,
                        unmount_ns: number(&mut fields)?,
                        wait_ns: number(&mut fields)?,
                        cleanup_ns: number(&mut fields)?,
                        unattributed_ns: number(&mut fields)?,
                        docker_calls: number(&mut fields)?,
                    },
                ),
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
                "{{\"local\":{{\"candidate_ids\":{},\"candidate_bytes\":{},\"inserted_ids\":{},\"inserted_bytes\":{},\"reused_ids\":{},\"reused_bytes\":{},\"source_reused_ids\":{},\"source_reused_bytes\":{},\"cdc_bytes_scanned\":{},\"encode_hash_invocations\":{}}}}}",
                value.candidate_ids,
                value.candidate_bytes,
                value.inserted_ids,
                value.inserted_bytes,
                value.reused_ids,
                value.reused_bytes,
                receipt.source_reused_ids,
                receipt.source_reused_bytes,
                receipt.cdc_bytes_scanned,
                receipt.encode_hash_invocations,
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
        layerfs_storage::StorageReceipt::Durability(receipt) => format!(
            "{{\"durability\":{{\"store_id\":\"{}\",\"role\":\"{}\",\"stable_ns\":{},\"checkpoint_ns\":{},\"database_fsync_ns\":{},\"directory_fsync_ns\":{},\"unattributed_ns\":{}}}}}",
            receipt.store_id,
            match receipt.role {
                layerfs_storage::StoreRole::LayerStack => "layerstack",
                layerfs_storage::StoreRole::Branch => "branch",
            },
            receipt.stable_ns,
            receipt.checkpoint_ns,
            receipt.database_fsync_ns,
            receipt.directory_fsync_ns,
            receipt.unattributed_ns,
        ),
        layerfs_storage::StorageReceipt::WorkspaceCommit(receipt) => format!(
            "{{\"workspace_commit\":{{\"total_ns\":{},\"pause_fence_ns\":{},\"quiesce_ns\":{},\"capture_ns\":{},\"capture_mode\":{},\"captured_files\":{},\"captured_bytes\":{},\"candidate_plan_ns\":{},\"dirty_compare_ns\":{},\"content_ns\":{},\"namespace_ns\":{},\"candidate_finish_ns\":{},\"local_admission_ns\":{},\"completeness_verify_ns\":{},\"publication_ns\":{},\"in_place_rebase_ns\":{},\"resume_ns\":{},\"unattributed_ns\":{}}}}}",
            receipt.total_ns,
            receipt.pause_fence_ns,
            receipt.quiesce_ns,
            receipt.capture_ns,
            receipt.capture_mode.map_or("null".to_owned(), |mode| format!(
                "\"{}\"",
                match mode {
                    layerfs_storage::CaptureMode::Live => "live",
                    layerfs_storage::CaptureMode::Materialized => "materialized",
                }
            )),
            receipt.captured_files,
            receipt.captured_bytes,
            receipt.candidate_plan_ns,
            receipt.dirty_compare_ns,
            receipt.content_ns,
            receipt.namespace_ns,
            receipt.candidate_finish_ns,
            receipt.local_admission_ns,
            receipt.completeness_verify_ns,
            receipt.publication_ns,
            receipt.in_place_rebase_ns,
            receipt.resume_ns,
            receipt.unattributed_ns,
        ),
        layerfs_storage::StorageReceipt::Push(receipt) => format!(
            "{{\"push_phases\":{{\"total_ns\":{},\"history_ns\":{},\"frontier_ns\":{},\"membership_ns\":{},\"source_read_auth_ns\":{},\"object_admission_ns\":{},\"fact_admission_ns\":{},\"authority_transition_verify_ns\":{},\"publication_ns\":{},\"durability_ns\":{},\"unattributed_ns\":{},\"endpoint_calls\":{}}}}}",
            receipt.total_ns,
            receipt.history_ns,
            receipt.frontier_ns,
            receipt.membership_ns,
            receipt.source_read_auth_ns,
            receipt.object_admission_ns,
            receipt.fact_admission_ns,
            receipt.authority_transition_verify_ns,
            receipt.publication_ns,
            receipt.durability_ns,
            receipt.unattributed_ns,
            receipt.endpoint_calls,
        ),
        layerfs_storage::StorageReceipt::Database(receipt) => format!(
            "{{\"database\":{{\"store_id\":\"{}\",\"role\":\"{}\",\"operation\":\"{}\",\"total_ns\":{},\"connection_wait_ns\":{},\"writer_acquire_ns\":{},\"statement_ns\":{},\"publication_ns\":{},\"commit_sync_ns\":{},\"unattributed_ns\":{},\"statement_count\":{},\"rows\":{},\"bytes\":{}}}}}",
            receipt.store_id,
            store_role_name(receipt.role),
            database_operation_name(receipt.operation),
            receipt.total_ns,
            receipt.connection_wait_ns,
            receipt.writer_acquire_ns,
            receipt.statement_ns,
            receipt.publication_ns,
            receipt.commit_sync_ns,
            receipt.unattributed_ns,
            receipt.statement_count,
            receipt.rows,
            receipt.bytes,
        ),
        layerfs_storage::StorageReceipt::WorkspaceLifecycle(receipt) => format!(
            "{{\"workspace_lifecycle\":{{\"kind\":\"{}\",\"total_ns\":{},\"proxy_ns\":{},\"docker_setup_ns\":{},\"helper_copy_ns\":{},\"mount_ready_ns\":{},\"unmount_ns\":{},\"wait_ns\":{},\"cleanup_ns\":{},\"unattributed_ns\":{},\"docker_calls\":{}}}}}",
            match receipt.kind {
                layerfs_storage::WorkspaceLifecycleKind::Attach => "attach",
                layerfs_storage::WorkspaceLifecycleKind::End => "end",
            },
            receipt.total_ns,
            receipt.proxy_ns,
            receipt.docker_setup_ns,
            receipt.helper_copy_ns,
            receipt.mount_ready_ns,
            receipt.unmount_ns,
            receipt.wait_ns,
            receipt.cleanup_ns,
            receipt.unattributed_ns,
            receipt.docker_calls,
        ),
    }
}

fn store_role_code(role: layerfs_storage::StoreRole) -> &'static str {
    match role {
        layerfs_storage::StoreRole::LayerStack => "s",
        layerfs_storage::StoreRole::Branch => "b",
    }
}

fn store_role_name(role: layerfs_storage::StoreRole) -> &'static str {
    match role {
        layerfs_storage::StoreRole::LayerStack => "layerstack",
        layerfs_storage::StoreRole::Branch => "branch",
    }
}

fn take_store_role<'a>(
    fields: &mut impl Iterator<Item = &'a str>,
) -> crate::MonitorResult<layerfs_storage::StoreRole> {
    match next(fields)? {
        "s" => Ok(layerfs_storage::StoreRole::LayerStack),
        "b" => Ok(layerfs_storage::StoreRole::Branch),
        _ => Err(crate::MonitorError::Integrity("database Store role")),
    }
}

fn database_operation_code(operation: layerfs_storage::DatabaseOperation) -> &'static str {
    match operation {
        layerfs_storage::DatabaseOperation::ObjectAdmission => "o",
        layerfs_storage::DatabaseOperation::FactAdmission => "f",
        layerfs_storage::DatabaseOperation::CommitCas => "c",
        layerfs_storage::DatabaseOperation::AuthorityPublish => "p",
    }
}

fn database_operation_name(operation: layerfs_storage::DatabaseOperation) -> &'static str {
    match operation {
        layerfs_storage::DatabaseOperation::ObjectAdmission => "object_admission",
        layerfs_storage::DatabaseOperation::FactAdmission => "fact_admission",
        layerfs_storage::DatabaseOperation::CommitCas => "commit_cas",
        layerfs_storage::DatabaseOperation::AuthorityPublish => "authority_publish",
    }
}

fn take_database_operation<'a>(
    fields: &mut impl Iterator<Item = &'a str>,
) -> crate::MonitorResult<layerfs_storage::DatabaseOperation> {
    match next(fields)? {
        "o" => Ok(layerfs_storage::DatabaseOperation::ObjectAdmission),
        "f" => Ok(layerfs_storage::DatabaseOperation::FactAdmission),
        "c" => Ok(layerfs_storage::DatabaseOperation::CommitCas),
        "p" => Ok(layerfs_storage::DatabaseOperation::AuthorityPublish),
        _ => Err(crate::MonitorError::Integrity("database operation")),
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

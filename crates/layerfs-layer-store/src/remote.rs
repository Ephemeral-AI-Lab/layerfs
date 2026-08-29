use crate::{
    transfer::{PublicationPage, PublicationSpool},
    LayerStore,
};
use layerfs_storage::{
    read_object_frames, read_value, write_frame_bytes, write_value, BranchRecord, CommitId,
    EndpointRequest, EndpointResponse, Fact, FactKind, FrameKind, LayerId, ObjectSource,
    RefOutcome, Result, StackAttestation, StackHistoryId, StackHistoryRecord, StackId,
    StorageError, StoreEndpoint, TransferExchange, TransferIntent,
};
use std::io::{Read, Write};

pub fn serve_once(
    store: &LayerStore,
    input: &mut impl Read,
    output: &mut impl Write,
) -> Result<()> {
    let request: EndpointRequest = read_value(input, FrameKind::Command)?;
    if let Err(error) = dispatch(store, request, input, output) {
        write_value(output, FrameKind::Reply, &Err::<EndpointResponse, _>(error))?;
    }
    Ok(())
}

fn dispatch(
    store: &LayerStore,
    request: EndpointRequest,
    input: &mut impl Read,
    output: &mut impl Write,
) -> Result<()> {
    match request {
        EndpointRequest::ReadObjects(ids) => {
            reply(
                output,
                EndpointResponse::Objects(store.db.object_descriptors(&ids)?),
            )?;
            store.visit_objects(&ids, &mut |object| {
                write_frame_bytes(output, FrameKind::Payload, &object.bytes)
            })
        }
        EndpointRequest::CommitPages(id) => {
            reply(
                output,
                EndpointResponse::BranchRecord(store.branch_record(id)?),
            )?;
            let wire = std::cell::RefCell::new(HistoryWire { input, output });
            store.visit_commits(
                id,
                &mut |kind, ids| wire.borrow_mut().membership(kind, ids),
                &mut |page| {
                    wire.borrow_mut()
                        .facts(page.iter().copied().map(Fact::Commit).collect())
                },
            )?;
            wire.into_inner().finish()
        }
        EndpointRequest::LayerHistoryPrefix {
            history_id,
            through,
        } => {
            reply(
                output,
                EndpointResponse::LayerHistoryRecord(store.layer_history_record(history_id)?),
            )?;
            let wire = std::cell::RefCell::new(HistoryWire { input, output });
            store.visit_layers(
                history_id,
                through,
                &mut |kind, ids| wire.borrow_mut().membership(kind, ids),
                &mut |page| {
                    wire.borrow_mut()
                        .facts(page.iter().copied().map(Fact::Layer).collect())
                },
            )?;
            wire.into_inner().finish()
        }
        EndpointRequest::StackHistoryPrefix {
            history_id,
            through,
        } => {
            reply(
                output,
                EndpointResponse::StackHistoryRecord(store.stack_history_record(history_id)?),
            )?;
            let wire = std::cell::RefCell::new(HistoryWire { input, output });
            store.visit_stacks(
                history_id,
                through,
                &mut |kind, ids| wire.borrow_mut().membership(kind, ids),
                &mut |page| {
                    wire.borrow_mut()
                        .facts(page.iter().copied().map(Fact::Stack).collect())
                },
            )?;
            wire.into_inner().finish()
        }
        request @ (EndpointRequest::TransferBeginBranch { .. }
        | EndpointRequest::TransferBeginStack { .. }) => {
            transfer_session(store, request, input, output)
        }
        EndpointRequest::Transfer { .. }
        | EndpointRequest::TransferEnd { .. }
        | EndpointRequest::TransferAbort => Err(StorageError::Integrity("wire transfer sequence")),
        request => reply(output, phase(store, request, input)?),
    }
}

fn phase(
    store: &LayerStore,
    request: EndpointRequest,
    _input: &mut impl Read,
) -> Result<EndpointResponse> {
    Ok(match request {
        EndpointRequest::BaseSnapshot(id) => {
            EndpointResponse::BaseSnapshot(store.base_snapshot(id)?)
        }
        EndpointRequest::CommonBase { left, right } => {
            EndpointResponse::BaseSnapshot(store.common_base(left, right)?)
        }
        EndpointRequest::BranchRecord(id) => {
            EndpointResponse::BranchRecord(store.branch_record(id)?)
        }
        EndpointRequest::LayerHistoryRecord(id) => {
            EndpointResponse::LayerHistoryRecord(store.layer_history_record(id)?)
        }
        EndpointRequest::StackHistoryHead(id) => {
            EndpointResponse::StackHistoryRecord(store.stack_history_record(id)?)
        }
        EndpointRequest::StackRecord(id) => EndpointResponse::StackRecord(store.stack_record(id)?),
        EndpointRequest::StoreIdentity => EndpointResponse::StoreIdentity(store.store_identity()?),
        EndpointRequest::AddStack {
            stack_history_id,
            branch_id,
            commit_id,
        } => EndpointResponse::StackAdd(store.add_stack(stack_history_id, branch_id, commit_id)?),
        EndpointRequest::PushStack(_) => return Err(StorageError::WrongSourceRoute),
        EndpointRequest::AddLayer {
            layer_history_id,
            source,
        } => EndpointResponse::LayerAdd(store.add_layer_to_history(layer_history_id, source)?),
        EndpointRequest::ReadObjects(_)
        | EndpointRequest::HistoryMissing(_)
        | EndpointRequest::TransferBeginBranch { .. }
        | EndpointRequest::TransferBeginStack { .. }
        | EndpointRequest::Transfer { .. }
        | EndpointRequest::TransferEnd { .. }
        | EndpointRequest::TransferAbort
        | EndpointRequest::CommitPages(_)
        | EndpointRequest::LayerHistoryPrefix { .. }
        | EndpointRequest::StackHistoryPrefix { .. } => unreachable!(),
    })
}

fn transfer_session(
    store: &LayerStore,
    request: EndpointRequest,
    input: &mut impl Read,
    output: &mut impl Write,
) -> Result<()> {
    let _permit = store.db.enter_operation()?;
    let mut attestation = StackAttestation::default();
    let mut publication: Option<PublicationSpool> = None;
    let mut publication_page: Option<PublicationPage> = None;
    let mut streamed = TransferExchange::membership(
        layerfs_storage::MissingBitmap::empty(),
        layerfs_storage::MissingBitmap::empty(),
    );
    let session = match request {
        EndpointRequest::TransferBeginBranch { branch, root } => {
            let (expected, up_to_date) = store.db.preflight_branch_push(branch)?;
            let status = if up_to_date {
                EndpointResponse::CommitRef(RefOutcome::UpToDate(
                    expected.ok_or(StorageError::MissingBaseData)?,
                ))
            } else if let Some(head) = expected {
                EndpointResponse::BranchRecord(BranchRecord {
                    head_commit_id: head,
                    ..branch
                })
            } else {
                EndpointResponse::Unit
            };
            reply(output, status)?;
            reply(
                output,
                EndpointResponse::Exchange(TransferExchange::membership(
                    store.db.missing_objects(&[root])?,
                    layerfs_storage::MissingBitmap::empty(),
                )),
            )?;
            if up_to_date {
                return Ok(());
            }
            TransferSession::Branch { branch, expected }
        }
        EndpointRequest::TransferBeginStack {
            history_id,
            base_layer_id,
            incoming,
            root,
        } => {
            let (expected, up_to_date) =
                store
                    .db
                    .preflight_stack_push(history_id, base_layer_id, incoming)?;
            let status = if up_to_date {
                EndpointResponse::StackRef(RefOutcome::UpToDate(
                    expected.ok_or(StorageError::MissingBaseData)?,
                ))
            } else if let Some(head) = expected {
                EndpointResponse::StackHistoryRecord(StackHistoryRecord {
                    id: history_id,
                    base_layer_id,
                    head_stack_id: head,
                })
            } else {
                EndpointResponse::Unit
            };
            reply(output, status)?;
            reply(
                output,
                EndpointResponse::Exchange(TransferExchange::membership(
                    store.db.missing_objects(&[root])?,
                    layerfs_storage::MissingBitmap::empty(),
                )),
            )?;
            if up_to_date {
                return Ok(());
            }
            TransferSession::Stack {
                history_id,
                base_layer_id,
                expected,
                incoming,
            }
        }
        _ => return Err(StorageError::Integrity("wire transfer sequence")),
    };
    loop {
        match read_value(input, FrameKind::Command)? {
            EndpointRequest::Transfer {
                objects,
                facts,
                object_ids,
                fact_kind,
                fact_ids,
            } => {
                let publication_batch = objects.is_empty()
                    && object_ids.is_empty()
                    && fact_kind.is_some()
                    && fact_ids.is_empty()
                    && !facts.is_empty();
                if publication_batch {
                    if !session.allows_publication(&facts, fact_kind) {
                        return Err(StorageError::Integrity("wire publication session"));
                    }
                    let page = publication_page
                        .as_mut()
                        .ok_or(StorageError::Integrity("publication announcement"))?;
                    if page.receive(store, &facts, &mut publication)? {
                        publication_page = None;
                    }
                    continue;
                }
                if publication_page.is_some() {
                    return Err(StorageError::Integrity("incomplete publication page"));
                }
                if fact_kind.is_none() != fact_ids.is_empty() {
                    return Err(StorageError::Integrity("wire fact announcement"));
                }
                if !session.allows_transfer(&facts, fact_kind) {
                    return Err(StorageError::Integrity("wire transfer facts"));
                }
                let objects = read_object_frames(input, objects)?;
                if let Some(kind) = fact_kind {
                    attestation.observe(kind, &fact_ids);
                }
                let mut exchange = store.db.transfer_exchange(
                    &objects,
                    &facts,
                    &object_ids,
                    fact_kind.map(|kind| (kind, fact_ids.as_slice())),
                    true,
                )?;
                if matches!(session, TransferSession::Stack { .. }) {
                    if let Some((kind, ids)) = fact_kind
                        .filter(|kind| matches!(kind, FactKind::Branch | FactKind::AddResult))
                        .map(|kind| (kind, fact_ids.as_slice()))
                    {
                        publication_page = PublicationPage::begin(
                            store,
                            kind,
                            ids,
                            exchange.missing().1,
                            &mut publication,
                        )?;
                    }
                }
                if object_ids.is_empty() && fact_kind.is_none() {
                    streamed.absorb(exchange)?;
                } else {
                    exchange.absorb(streamed)?;
                    streamed = TransferExchange::membership(
                        layerfs_storage::MissingBitmap::empty(),
                        layerfs_storage::MissingBitmap::empty(),
                    );
                    reply(output, EndpointResponse::Exchange(exchange))?;
                }
            }
            EndpointRequest::TransferEnd {
                objects,
                facts,
                intent,
            } => {
                if publication_page.is_some() || !session.allows_final(&facts, intent.as_ref()) {
                    return Err(StorageError::Integrity("wire transfer final"));
                }
                let objects = read_object_frames(input, objects)?;
                let (mut exchange, outcome) = store.finish_received_transfer(
                    &objects,
                    &facts,
                    *intent,
                    attestation,
                    publication,
                )?;
                exchange.absorb(streamed)?;
                return reply(output, EndpointResponse::TransferDone { exchange, outcome });
            }
            EndpointRequest::TransferAbort => return Ok(()),
            _ => return Err(StorageError::Integrity("wire transfer sequence")),
        }
    }
}

enum TransferSession {
    Branch {
        branch: BranchRecord,
        expected: Option<CommitId>,
    },
    Stack {
        history_id: StackHistoryId,
        base_layer_id: LayerId,
        expected: Option<StackId>,
        incoming: StackId,
    },
}

impl TransferSession {
    fn allows_transfer(&self, facts: &[Fact], announcement: Option<FactKind>) -> bool {
        match self {
            Self::Branch { .. } => {
                facts.iter().all(|fact| fact.kind() == FactKind::Commit)
                    && announcement.is_none_or(|kind| kind == FactKind::Commit)
            }
            Self::Stack { .. } => {
                facts
                    .iter()
                    .all(|fact| matches!(fact.kind(), FactKind::Commit | FactKind::Stack))
                    && announcement.is_none_or(|kind| {
                        matches!(
                            kind,
                            FactKind::Commit
                                | FactKind::Stack
                                | FactKind::Branch
                                | FactKind::AddResult
                        )
                    })
            }
        }
    }

    fn allows_publication(&self, facts: &[Fact], kind: Option<FactKind>) -> bool {
        matches!(self, Self::Stack { .. })
            && kind.is_some_and(|kind| matches!(kind, FactKind::Branch | FactKind::AddResult))
            && facts.iter().all(|fact| Some(fact.kind()) == kind)
    }

    fn allows_final(&self, facts: &[Fact], intent: &TransferIntent) -> bool {
        if !self.allows_transfer(facts, None) {
            return false;
        }
        match (self, intent) {
            (
                Self::Branch { branch, expected },
                TransferIntent::Branch {
                    branch: value,
                    expected: value_expected,
                },
            ) => value == branch && value_expected == expected,
            (
                Self::Stack {
                    history_id,
                    base_layer_id,
                    expected,
                    incoming,
                },
                TransferIntent::Stack(push),
            ) => {
                push.history_id == *history_id
                    && push.base_layer_id == *base_layer_id
                    && push.expected_head == *expected
                    && push.incoming_head == *incoming
            }
            _ => false,
        }
    }
}

struct HistoryWire<'a, R: Read, W: Write> {
    input: &'a mut R,
    output: &'a mut W,
}

impl<R: Read, W: Write> HistoryWire<'_, R, W> {
    fn membership(
        &mut self,
        kind: layerfs_storage::FactKind,
        ids: &[Vec<u8>],
    ) -> Result<layerfs_storage::MissingBitmap> {
        reply(
            self.output,
            EndpointResponse::FactIds {
                kind,
                ids: ids.to_vec(),
            },
        )?;
        match read_value(self.input, FrameKind::Command)? {
            EndpointRequest::HistoryMissing(missing) => Ok(missing),
            _ => Err(StorageError::Integrity("wire history membership")),
        }
    }

    fn facts(&mut self, facts: Vec<Fact>) -> Result<()> {
        reply(self.output, EndpointResponse::Facts(facts))
    }

    fn finish(self) -> Result<()> {
        reply(self.output, EndpointResponse::Unit)
    }
}

fn reply(output: &mut impl Write, response: EndpointResponse) -> Result<()> {
    write_value(output, FrameKind::Reply, &Ok::<_, StorageError>(response))
}

use crate::StackStore;
use layerfs_storage::{
    read_object_frames, read_value, write_frame_bytes, write_value, BranchRecord, EndpointRequest,
    EndpointResponse, Fact, FactKind, FrameKind, ObjectSource, RefOutcome, Result, StorageError,
    StoreEndpoint, TransferExchange, TransferIntent,
};
use std::io::{Read, Write};

pub fn serve_once(
    store: &StackStore,
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
    store: &StackStore,
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
        request @ EndpointRequest::TransferBeginBranch { .. } => {
            transfer_session(store, request, input, output)
        }
        EndpointRequest::TransferBeginStack { .. } => Err(StorageError::WrongSourceRoute),
        EndpointRequest::Transfer { .. }
        | EndpointRequest::TransferEnd { .. }
        | EndpointRequest::TransferAbort => Err(StorageError::Integrity("wire transfer sequence")),
        request => reply(output, phase(store, request, input)?),
    }
}

fn phase(
    store: &StackStore,
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
        } => EndpointResponse::StackAdd(store.add_stack_to_history(
            stack_history_id,
            branch_id,
            commit_id,
        )?),
        EndpointRequest::PushStack(id) => EndpointResponse::StackRef(store.push_stack(id)?),
        EndpointRequest::AddLayer {
            layer_history_id,
            source,
        } => EndpointResponse::LayerAdd(store.add_layer(layer_history_id, source)?),
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
    store: &StackStore,
    request: EndpointRequest,
    input: &mut impl Read,
    output: &mut impl Write,
) -> Result<()> {
    let _permit = store.db.enter_operation()?;
    let EndpointRequest::TransferBeginBranch { branch, root } = request else {
        return Err(StorageError::Integrity("wire transfer sequence"));
    };
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
    let mut streamed = TransferExchange::membership(
        layerfs_storage::MissingBitmap::empty(),
        layerfs_storage::MissingBitmap::empty(),
    );
    loop {
        match read_value(input, FrameKind::Command)? {
            EndpointRequest::Transfer {
                objects,
                facts,
                object_ids,
                fact_kind,
                fact_ids,
            } => {
                if fact_kind.is_none() != fact_ids.is_empty() {
                    return Err(StorageError::Integrity("wire fact announcement"));
                }
                if facts.iter().any(|fact| fact.kind() != FactKind::Commit)
                    || fact_kind.is_some_and(|kind| kind != FactKind::Commit)
                {
                    return Err(StorageError::Integrity("wire Branch facts"));
                }
                let objects = read_object_frames(input, objects)?;
                let mut exchange = store.db.transfer_exchange(
                    &objects,
                    &facts,
                    &object_ids,
                    fact_kind.map(|kind| (kind, fact_ids.as_slice())),
                    true,
                )?;
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
                if facts.iter().any(|fact| fact.kind() != FactKind::Commit)
                    || !matches!(
                        intent.as_ref(),
                        TransferIntent::Branch { branch: value, expected: value_expected }
                            if *value == branch && *value_expected == expected
                    )
                {
                    return Err(StorageError::Integrity("wire Branch final"));
                }
                let objects = read_object_frames(input, objects)?;
                let (mut exchange, outcome) =
                    store.db.finish_transfer(&objects, &facts, *intent)?;
                exchange.absorb(streamed)?;
                return reply(output, EndpointResponse::TransferDone { exchange, outcome });
            }
            EndpointRequest::TransferAbort => return Ok(()),
            _ => return Err(StorageError::Integrity("wire transfer sequence")),
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

mod client {
    use layerfs_content::ObjectId;
    use layerfs_storage::{
        read_frame, read_payload_bytes, read_value, write_frame_bytes, write_value, AddResult,
        BaseId, BaseSnapshot, BranchId, BranchRecord, CanonicalObject, CommitId, CommitRecord,
        EndpointReply, EndpointRequest, EndpointResponse, Fact, FactKind, FrameKind,
        LayerHistoryId, LayerHistoryRecord, LayerId, LayerRecord, LayerSource, ObjectSource,
        RefOutcome, Result, StackHistoryId, StackHistoryRecord, StackId, StackRecord, StorageError,
        StoreEndpoint, TransferExchange, TransferIntent, TransferOutcome, TransferTarget,
    };
    use std::net::{SocketAddr, TcpStream};
    use std::sync::{Arc, Mutex, MutexGuard};

    type Membership<'a> =
        dyn FnMut(FactKind, &[Vec<u8>]) -> Result<layerfs_storage::MissingBitmap> + 'a;

    /// Opaque remote Store connection. Raw object/fact phases are not exposed.
    ///
    /// ```compile_fail
    /// # use layerfs_stack_store::RemoteEndpoint;
    /// fn raw_store(endpoint: &RemoteEndpoint) {
    ///     let _ = endpoint.store();
    /// }
    /// ```
    #[derive(Clone)]
    pub struct RemoteEndpoint {
        stream: Arc<Mutex<TcpStream>>,
    }

    impl RemoteEndpoint {
        pub fn connect(address: SocketAddr) -> Result<Self> {
            let stream = TcpStream::connect(address)?;
            stream.set_nodelay(true)?;
            Ok(Self {
                stream: Arc::new(Mutex::new(stream)),
            })
        }

        pub fn add_stack(
            &self,
            stack_history_id: StackHistoryId,
            branch_id: BranchId,
            commit_id: CommitId,
        ) -> Result<AddResult<StackId>> {
            match self
                .request(
                    EndpointRequest::AddStack {
                        stack_history_id,
                        branch_id,
                        commit_id,
                    },
                    &[],
                )?
                .0
            {
                EndpointResponse::StackAdd(value) => Ok(value),
                _ => Err(StorageError::Integrity("wire Add Stack reply")),
            }
        }

        pub fn push_stack(&self, stack_id: StackId) -> Result<RefOutcome<StackId>> {
            match self.request(EndpointRequest::PushStack(stack_id), &[])?.0 {
                EndpointResponse::StackRef(value) => Ok(value),
                _ => Err(StorageError::Integrity("wire Push Stack reply")),
            }
        }

        pub fn add_layer(
            &self,
            layer_history_id: LayerHistoryId,
            source: LayerSource,
        ) -> Result<AddResult<LayerId>> {
            match self
                .request(
                    EndpointRequest::AddLayer {
                        layer_history_id,
                        source,
                    },
                    &[],
                )?
                .0
            {
                EndpointResponse::LayerAdd(value) => Ok(value),
                _ => Err(StorageError::Integrity("wire Add Layer reply")),
            }
        }

        fn request(
            &self,
            request: EndpointRequest,
            payloads: &[CanonicalObject],
        ) -> Result<(EndpointResponse, Vec<Vec<u8>>)> {
            let mut stream = self
                .stream
                .lock()
                .map_err(|_| StorageError::Integrity("remote stream"))?;
            Self::request_on(&mut stream, request, payloads)
        }

        fn request_on(
            stream: &mut TcpStream,
            request: EndpointRequest,
            payloads: &[CanonicalObject],
        ) -> Result<(EndpointResponse, Vec<Vec<u8>>)> {
            Self::send_on(stream, request, payloads)?;
            let reply: EndpointReply = read_value(&mut *stream, FrameKind::Reply)?;
            let response = reply?;
            let mut payloads = Vec::new();
            if let EndpointResponse::Objects(objects) = &response {
                for _ in objects {
                    let frame = read_frame(&mut *stream)?;
                    if frame.kind != FrameKind::Payload {
                        return Err(StorageError::Integrity("wire object sequence"));
                    }
                    payloads.push(frame.bytes);
                }
            }
            Ok((response, payloads))
        }

        fn send_on(
            stream: &mut TcpStream,
            request: EndpointRequest,
            payloads: &[CanonicalObject],
        ) -> Result<()> {
            write_value(&mut *stream, FrameKind::Command, &request)?;
            for object in payloads {
                write_frame_bytes(&mut *stream, FrameKind::Payload, &object.bytes)?;
            }
            Ok(())
        }

        fn stream_history(
            &self,
            request: EndpointRequest,
            header: impl FnOnce(&EndpointResponse) -> bool,
            membership: &mut Membership<'_>,
            visitor: &mut dyn FnMut(&[Fact]) -> Result<()>,
        ) -> Result<()> {
            let mut stream = self
                .stream
                .lock()
                .map_err(|_| StorageError::Integrity("remote stream"))?;
            write_value(&mut *stream, FrameKind::Command, &request)?;
            let first: EndpointReply = read_value(&mut *stream, FrameKind::Reply)?;
            if !header(&first?) {
                return Err(StorageError::Integrity("wire history header"));
            }
            loop {
                let reply: EndpointReply = read_value(&mut *stream, FrameKind::Reply)?;
                match reply? {
                    EndpointResponse::FactIds { kind, ids } => {
                        let missing = membership(kind, &ids)?;
                        missing.validate_tail(ids.len())?;
                        write_value(
                            &mut *stream,
                            FrameKind::Command,
                            &EndpointRequest::HistoryMissing(missing),
                        )?;
                        let rows: EndpointReply = read_value(&mut *stream, FrameKind::Reply)?;
                        let EndpointResponse::Facts(facts) = rows? else {
                            return Err(StorageError::Integrity("wire history rows"));
                        };
                        visitor(&facts)?;
                    }
                    EndpointResponse::Unit => return Ok(()),
                    _ => return Err(StorageError::Integrity("wire history page")),
                }
            }
        }
    }

    impl ObjectSource for RemoteEndpoint {
        fn read_object(&self, id: ObjectId) -> Result<Vec<u8>> {
            Ok(self.read_objects(&[id])?.remove(0).bytes)
        }

        fn read_objects(&self, ids: &[ObjectId]) -> Result<Vec<CanonicalObject>> {
            let mut rows = std::collections::BTreeMap::new();
            self.visit_objects(ids, &mut |object| {
                rows.insert(object.id, object.bytes);
                Ok(())
            })?;
            ids.iter()
                .map(|id| {
                    Ok(CanonicalObject {
                        id: *id,
                        bytes: rows.remove(id).ok_or(StorageError::MissingBaseData)?,
                    })
                })
                .collect()
        }

        fn visit_objects(
            &self,
            ids: &[ObjectId],
            visitor: &mut dyn FnMut(CanonicalObject) -> Result<()>,
        ) -> Result<()> {
            let mut stream = self
                .stream
                .lock()
                .map_err(|_| StorageError::Integrity("remote stream"))?;
            write_value(
                &mut *stream,
                FrameKind::Command,
                &EndpointRequest::ReadObjects(ids.to_vec()),
            )?;
            let reply: EndpointReply = read_value(&mut *stream, FrameKind::Reply)?;
            let EndpointResponse::Objects(frames) = reply? else {
                return Err(StorageError::Integrity("wire object reply"));
            };
            if frames.len() != ids.len() {
                return Err(StorageError::Integrity("wire object count"));
            }
            let mut expected = ids
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>();
            for (id, len) in frames {
                if !expected.remove(&id) {
                    return Err(StorageError::Integrity("wire object descriptor"));
                }
                let object = CanonicalObject {
                    id,
                    bytes: read_payload_bytes(&mut *stream, len)?,
                };
                visitor(object)?;
            }
            if expected.is_empty() {
                Ok(())
            } else {
                Err(StorageError::MissingBaseData)
            }
        }
    }

    impl StoreEndpoint for RemoteEndpoint {
        fn store_identity(&self) -> Result<[u8; 32]> {
            match self.request(EndpointRequest::StoreIdentity, &[])?.0 {
                EndpointResponse::StoreIdentity(value) => Ok(value),
                _ => Err(StorageError::Integrity("wire Store identity reply")),
            }
        }

        fn begin_transfer(&self) -> Result<Box<dyn TransferTarget + '_>> {
            Ok(Box::new(RemoteTransfer {
                stream: self
                    .stream
                    .lock()
                    .map_err(|_| StorageError::Integrity("remote stream"))?,
                started: false,
                finished: false,
            }))
        }

        fn transfer_exchange_unlocked(
            &self,
            objects: &[CanonicalObject],
            facts: &[Fact],
            object_ids: &[ObjectId],
            fact_ids: Option<(FactKind, &[Vec<u8>])>,
        ) -> Result<TransferExchange> {
            let frames = objects
                .iter()
                .map(|object| (object.id, object.bytes.len() as u64))
                .collect();
            let (fact_kind, fact_ids) = fact_ids
                .map(|(kind, ids)| (Some(kind), ids.to_vec()))
                .unwrap_or((None, Vec::new()));
            match self
                .request(
                    EndpointRequest::Transfer {
                        objects: frames,
                        facts: facts.to_vec(),
                        object_ids: object_ids.to_vec(),
                        fact_kind,
                        fact_ids,
                    },
                    objects,
                )?
                .0
            {
                EndpointResponse::Exchange(value) => Ok(value),
                _ => Err(StorageError::Integrity("wire transfer reply")),
            }
        }

        fn base_snapshot(&self, base_id: BaseId) -> Result<BaseSnapshot> {
            base(self.request(EndpointRequest::BaseSnapshot(base_id), &[])?.0)
        }

        fn common_base(&self, left: BaseId, right: BaseId) -> Result<BaseSnapshot> {
            base(
                self.request(EndpointRequest::CommonBase { left, right }, &[])?
                    .0,
            )
        }

        fn branch_record(&self, branch_id: BranchId) -> Result<BranchRecord> {
            match self
                .request(EndpointRequest::BranchRecord(branch_id), &[])?
                .0
            {
                EndpointResponse::BranchRecord(value) => Ok(value),
                _ => Err(StorageError::Integrity("wire Branch reply")),
            }
        }

        fn visit_commits(
            &self,
            branch_id: BranchId,
            membership: &mut dyn FnMut(
                FactKind,
                &[Vec<u8>],
            ) -> Result<layerfs_storage::MissingBitmap>,
            visitor: &mut dyn FnMut(&[CommitRecord]) -> Result<()>,
        ) -> Result<()> {
            self.stream_history(
                EndpointRequest::CommitPages(branch_id),
                |response| matches!(response, EndpointResponse::BranchRecord(_)),
                membership,
                &mut |facts| visitor(&commit_page(facts)?),
            )
        }

        fn layer_history_record(&self, history_id: LayerHistoryId) -> Result<LayerHistoryRecord> {
            match self
                .request(EndpointRequest::LayerHistoryRecord(history_id), &[])?
                .0
            {
                EndpointResponse::LayerHistoryRecord(value) => Ok(value),
                _ => Err(StorageError::Integrity("wire LayerHistory reply")),
            }
        }

        fn visit_layers(
            &self,
            history_id: LayerHistoryId,
            through: LayerId,
            membership: &mut dyn FnMut(
                FactKind,
                &[Vec<u8>],
            ) -> Result<layerfs_storage::MissingBitmap>,
            visitor: &mut dyn FnMut(&[LayerRecord]) -> Result<()>,
        ) -> Result<()> {
            self.stream_history(
                EndpointRequest::LayerHistoryPrefix {
                    history_id,
                    through,
                },
                |response| matches!(response, EndpointResponse::LayerHistoryRecord(_)),
                membership,
                &mut |facts| visitor(&layer_page(facts)?),
            )
        }

        fn stack_history_record(&self, history_id: StackHistoryId) -> Result<StackHistoryRecord> {
            match self
                .request(EndpointRequest::StackHistoryHead(history_id), &[])?
                .0
            {
                EndpointResponse::StackHistoryRecord(value) => Ok(value),
                _ => Err(StorageError::Integrity("wire StackHistory reply")),
            }
        }

        fn stack_record(&self, stack_id: StackId) -> Result<StackRecord> {
            match self.request(EndpointRequest::StackRecord(stack_id), &[])?.0 {
                EndpointResponse::StackRecord(value) => Ok(value),
                _ => Err(StorageError::Integrity("wire Stack reply")),
            }
        }

        fn visit_stacks(
            &self,
            history_id: StackHistoryId,
            through: StackId,
            membership: &mut dyn FnMut(
                FactKind,
                &[Vec<u8>],
            ) -> Result<layerfs_storage::MissingBitmap>,
            visitor: &mut dyn FnMut(&[StackRecord]) -> Result<()>,
        ) -> Result<()> {
            self.stream_history(
                EndpointRequest::StackHistoryPrefix {
                    history_id,
                    through,
                },
                |response| matches!(response, EndpointResponse::StackHistoryRecord(_)),
                membership,
                &mut |facts| visitor(&stack_page(facts)?),
            )
        }
    }

    struct RemoteTransfer<'a> {
        stream: MutexGuard<'a, TcpStream>,
        started: bool,
        finished: bool,
    }

    impl TransferTarget for RemoteTransfer<'_> {
        fn preflight_branch(
            &mut self,
            branch: BranchRecord,
            root: ObjectId,
        ) -> Result<(Option<CommitId>, bool, layerfs_storage::MissingBitmap)> {
            let status = RemoteEndpoint::request_on(
                &mut self.stream,
                EndpointRequest::TransferBeginBranch { branch, root },
                &[],
            )?
            .0;
            self.started = true;
            let (current, up_to_date) = match status {
                EndpointResponse::CommitRef(RefOutcome::UpToDate(id)) => (Some(id), true),
                EndpointResponse::BranchRecord(record) => (Some(record.head_commit_id), false),
                EndpointResponse::Unit => (None, false),
                _ => return Err(StorageError::Integrity("wire Branch preflight")),
            };
            let reply: EndpointReply = read_value(&mut *self.stream, FrameKind::Reply)?;
            let EndpointResponse::Exchange(exchange) = reply? else {
                return Err(StorageError::Integrity("wire preflight membership"));
            };
            let (objects, facts) = exchange.missing();
            if facts != layerfs_storage::MissingBitmap::empty() {
                return Err(StorageError::Integrity("wire preflight facts"));
            }
            self.finished = up_to_date;
            Ok((current, up_to_date, objects))
        }

        fn preflight_stack(
            &mut self,
            history_id: StackHistoryId,
            base_layer_id: LayerId,
            incoming: StackId,
            root: ObjectId,
        ) -> Result<(Option<StackId>, bool, layerfs_storage::MissingBitmap)> {
            let status = RemoteEndpoint::request_on(
                &mut self.stream,
                EndpointRequest::TransferBeginStack {
                    history_id,
                    base_layer_id,
                    incoming,
                    root,
                },
                &[],
            )?
            .0;
            self.started = true;
            let (current, up_to_date) = match status {
                EndpointResponse::StackRef(RefOutcome::UpToDate(id)) => (Some(id), true),
                EndpointResponse::StackHistoryRecord(record) => (Some(record.head_stack_id), false),
                EndpointResponse::Unit => (None, false),
                _ => return Err(StorageError::Integrity("wire Stack preflight")),
            };
            let reply: EndpointReply = read_value(&mut *self.stream, FrameKind::Reply)?;
            let EndpointResponse::Exchange(exchange) = reply? else {
                return Err(StorageError::Integrity("wire preflight membership"));
            };
            let (objects, facts) = exchange.missing();
            if facts != layerfs_storage::MissingBitmap::empty() {
                return Err(StorageError::Integrity("wire preflight facts"));
            }
            self.finished = up_to_date;
            Ok((current, up_to_date, objects))
        }

        fn exchange(
            &mut self,
            objects: &[CanonicalObject],
            facts: &[Fact],
            object_ids: &[ObjectId],
            fact_ids: Option<(FactKind, &[Vec<u8>])>,
        ) -> Result<TransferExchange> {
            let frames = objects
                .iter()
                .map(|object| (object.id, object.bytes.len() as u64))
                .collect();
            let (fact_kind, fact_ids) = fact_ids
                .map(|(kind, ids)| (Some(kind), ids.to_vec()))
                .unwrap_or((None, Vec::new()));
            let payload_only = object_ids.is_empty() && fact_kind.is_none();
            let request = EndpointRequest::Transfer {
                objects: frames,
                facts: facts.to_vec(),
                object_ids: object_ids.to_vec(),
                fact_kind,
                fact_ids,
            };
            if payload_only {
                RemoteEndpoint::send_on(&mut self.stream, request, objects)?;
                self.started = true;
                return Ok(TransferExchange::membership(
                    layerfs_storage::MissingBitmap::empty(),
                    layerfs_storage::MissingBitmap::empty(),
                ));
            }
            let response = RemoteEndpoint::request_on(&mut self.stream, request, objects)?.0;
            self.started = true;
            match response {
                EndpointResponse::Exchange(exchange) => Ok(exchange),
                _ => Err(StorageError::Integrity("wire transfer reply")),
            }
        }

        fn defer_publication(&mut self, facts: &[Fact]) -> Result<()> {
            let kind = facts
                .first()
                .map(|fact| fact.kind())
                .ok_or(StorageError::InvalidInput("publication batch"))?;
            if facts.iter().any(|fact| fact.kind() != kind) {
                return Err(StorageError::InvalidInput("publication batch"));
            }
            RemoteEndpoint::send_on(
                &mut self.stream,
                EndpointRequest::Transfer {
                    objects: Vec::new(),
                    facts: facts.to_vec(),
                    object_ids: Vec::new(),
                    fact_kind: Some(kind),
                    fact_ids: Vec::new(),
                },
                &[],
            )?;
            self.started = true;
            Ok(())
        }

        fn finish(
            mut self: Box<Self>,
            objects: &[CanonicalObject],
            facts: &[Fact],
            intent: TransferIntent,
        ) -> Result<(TransferExchange, TransferOutcome)> {
            let frames = objects
                .iter()
                .map(|object| (object.id, object.bytes.len() as u64))
                .collect();
            let response = RemoteEndpoint::request_on(
                &mut self.stream,
                EndpointRequest::TransferEnd {
                    objects: frames,
                    facts: facts.to_vec(),
                    intent: Box::new(intent),
                },
                objects,
            )?
            .0;
            self.finished = true;
            match response {
                EndpointResponse::TransferDone { exchange, outcome } => Ok((exchange, outcome)),
                _ => Err(StorageError::Integrity("wire transfer final reply")),
            }
        }
    }

    impl Drop for RemoteTransfer<'_> {
        fn drop(&mut self) {
            if self.started && !self.finished {
                let _ = write_value(
                    &mut *self.stream,
                    FrameKind::Command,
                    &EndpointRequest::TransferAbort,
                );
            }
        }
    }

    fn commit_page(facts: &[Fact]) -> Result<Vec<CommitRecord>> {
        facts
            .iter()
            .map(|fact| match fact {
                Fact::Commit(value) => Ok(*value),
                _ => Err(StorageError::Integrity("wire Commit page")),
            })
            .collect()
    }

    fn layer_page(facts: &[Fact]) -> Result<Vec<LayerRecord>> {
        facts
            .iter()
            .map(|fact| match fact {
                Fact::Layer(value) => Ok(*value),
                _ => Err(StorageError::Integrity("wire Layer page")),
            })
            .collect()
    }

    fn stack_page(facts: &[Fact]) -> Result<Vec<StackRecord>> {
        facts
            .iter()
            .map(|fact| match fact {
                Fact::Stack(value) => Ok(*value),
                _ => Err(StorageError::Integrity("wire Stack page")),
            })
            .collect()
    }

    fn base(response: EndpointResponse) -> Result<BaseSnapshot> {
        match response {
            EndpointResponse::BaseSnapshot(value) => Ok(value),
            _ => Err(StorageError::Integrity("wire base reply")),
        }
    }
}

pub use client::RemoteEndpoint;

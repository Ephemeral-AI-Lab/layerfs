#[test]
fn extent_visitor_streams_the_mapping_without_fetching_payload_bytes() {
    let bytes = (0..1_000_000)
        .map(|index| (index as u64).wrapping_mul(0x9e37_79b9) as u8)
        .collect::<Vec<_>>();
    let mut store = MemoryStore::default();
    let (root, built) = build(&mut store, bytes.as_slice()).unwrap();
    let mut extents = 0_u64;
    let mut logical = 0_u64;
    let (state, visited) = visit_extents(&store, root, |batch| {
        extents += batch.len() as u64;
        logical += batch
            .iter()
            .map(|extent| u64::from(extent.logical_length))
            .sum::<u64>();
        Ok(())
    })
    .unwrap();
    assert_eq!(extents, state.extent_count);
    assert_eq!(logical, state.logical_len);
    assert_eq!(visited.payload_bytes_read, 0);
    assert_eq!(visited.cdc_bytes_scanned, 0);
    assert!(visited.nodes_read <= built.nodes_created + 1);
}

struct BatchRead<'a> {
    store: &'a MemoryStore,
    batches: Cell<u64>,
    state_root: ObjectId,
    state_reads: Cell<u64>,
}

impl ObjectRead for BatchRead<'_> {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        if id == self.state_root {
            self.state_reads.set(self.state_reads.get() + 1);
        }
        ObjectStore::get(self.store, id)
    }

    fn get_authenticated_batch<F>(&self, ids: &[ObjectId], mut callback: F) -> CoreResult<()>
    where
        F: FnMut(ObjectId, &[u8]) -> CoreResult<()>,
    {
        assert!(!ids.is_empty() && ids.len() <= 64);
        self.batches.set(self.batches.get() + 1);
        for id in ids {
            let bytes = self.store.0.get(id).ok_or(CoreError::MissingObject)?;
            if ObjectId::for_bytes(bytes) != *id {
                return Err(CoreError::IdentityMismatch);
            }
            callback(*id, decode_bytes_object(bytes)?)?;
        }
        Ok(())
    }
}

struct CountRead<'a> {
    store: &'a MemoryStore,
    reads: Cell<u64>,
}

impl ObjectRead for CountRead<'_> {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        self.reads.set(self.reads.get() + 1);
        ObjectStore::get(self.store, id)
    }
}

struct MissingBatchCallback<'a>(&'a MemoryStore);

impl ObjectRead for MissingBatchCallback<'_> {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        ObjectStore::get(self.0, id)
    }

    fn get_authenticated_batch<F>(&self, _ids: &[ObjectId], _callback: F) -> CoreResult<()>
    where
        F: FnMut(ObjectId, &[u8]) -> CoreResult<()>,
    {
        Ok(())
    }
}

struct ExtraBatchCallback<'a>(&'a MemoryStore);

impl ObjectRead for ExtraBatchCallback<'_> {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        ObjectStore::get(self.0, id)
    }

    fn get_authenticated_batch<F>(&self, ids: &[ObjectId], mut callback: F) -> CoreResult<()>
    where
        F: FnMut(ObjectId, &[u8]) -> CoreResult<()>,
    {
        for id in ids {
            let canonical = self.0 .0.get(id).ok_or(CoreError::MissingObject)?;
            callback(*id, decode_bytes_object(canonical)?)?;
        }
        let id = *ids.last().ok_or(CoreError::MissingObject)?;
        callback(id, decode_bytes_object(self.0 .0.get(&id).unwrap())?)
    }
}

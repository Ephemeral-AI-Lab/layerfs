//! Work performed while reconstructing pooled metadata.

/// Cumulative pooled-reader work; read APIs report successful waves only.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PoolReadCounters {
    /// Canonical pooled leaves requested.
    pub leaf_requests: u64,
    /// Pooled dependency edges followed.
    pub chain_edges: u64,
    /// Physical pooled records extracted, including both chain passes.
    pub physical_record_calls: u64,
    /// Compressed physical leaf groups actually decompressed.
    pub physical_group_decodes: u64,
    /// Bytes produced by physical leaf-group decompression.
    pub physical_group_decoded_bytes: u64,
    /// Physical leaf-group decompressions avoided by cached bodies.
    pub physical_group_cache_hits: u64,
    /// Stored-value lookups made: one statement execution each.
    pub value_lookups: u64,
    /// Values answered by the reader's retained values, without a lookup.
    pub value_cache_hits: u64,
    /// Retained values dropped for the existing byte allowance.
    pub value_cache_evictions: u64,
    /// Stored values returned by lookups.
    pub value_rows: u64,
    /// Real pooled source acquisitions, including private immutable views.
    pub pack_fetches: u64,
    /// Complete source body/scan bytes charged to acquisitions, not device I/O.
    pub pack_bytes: u64,
    /// Canonical inode-table branches reconstructed: one chain each (#360 item 3).
    pub branch_requests: u64,
    /// Branch dependency edges those chains followed.
    pub branch_chain_edges: u64,
    /// Branch delta programs applied.
    pub branch_delta_applies: u64,
    /// Directory and extent pages reconstructed: one chain each (#92). The
    /// `branch_*` counts above stay inode-table branches only.
    pub page_requests: u64,
    /// Dependency edges those page chains followed.
    pub page_chain_edges: u64,
    /// Page delta programs applied.
    pub page_delta_applies: u64,
    /// Canonical bytes every program-page chain rebuilt (inode branches,
    /// directory and extent pages), every element included: divided by the
    /// requests, the decode bytes per page read (#92).
    pub program_chain_canonical: u64,
    /// Stored record bytes every program-page chain read, every element included.
    pub program_chain_encoded: u64,
    /// Deepest program-page chain reconstructed.
    pub program_max_depth: u64,
}

impl PoolReadCounters {
    /// Zero work, also usable when constructing a provider in a const context.
    pub const fn new() -> Self {
        Self {
            leaf_requests: 0,
            chain_edges: 0,
            physical_record_calls: 0,
            physical_group_decodes: 0,
            physical_group_decoded_bytes: 0,
            physical_group_cache_hits: 0,
            value_lookups: 0,
            value_cache_hits: 0,
            value_cache_evictions: 0,
            value_rows: 0,
            pack_fetches: 0,
            pack_bytes: 0,
            branch_requests: 0,
            branch_chain_edges: 0,
            branch_delta_applies: 0,
            page_requests: 0,
            page_chain_edges: 0,
            page_delta_applies: 0,
            program_chain_canonical: 0,
            program_chain_encoded: 0,
            program_max_depth: 0,
        }
    }

    /// Work performed since an earlier reading of the same reader.
    ///
    /// A reader whose lifetime is its caller's operation reports cumulative work,
    /// while a chain wants only its own share. Subtracting the reading taken when
    /// the chain began is that share, and it is saturating so a shared reader can
    /// never report negative work for a chain that charged nothing.
    pub fn since(self, before: Self) -> Self {
        Self {
            leaf_requests: self.leaf_requests.saturating_sub(before.leaf_requests),
            chain_edges: self.chain_edges.saturating_sub(before.chain_edges),
            physical_record_calls: self
                .physical_record_calls
                .saturating_sub(before.physical_record_calls),
            physical_group_decodes: self
                .physical_group_decodes
                .saturating_sub(before.physical_group_decodes),
            physical_group_decoded_bytes: self
                .physical_group_decoded_bytes
                .saturating_sub(before.physical_group_decoded_bytes),
            physical_group_cache_hits: self
                .physical_group_cache_hits
                .saturating_sub(before.physical_group_cache_hits),
            value_lookups: self.value_lookups.saturating_sub(before.value_lookups),
            value_cache_hits: self
                .value_cache_hits
                .saturating_sub(before.value_cache_hits),
            value_cache_evictions: self
                .value_cache_evictions
                .saturating_sub(before.value_cache_evictions),
            value_rows: self.value_rows.saturating_sub(before.value_rows),
            pack_fetches: self.pack_fetches.saturating_sub(before.pack_fetches),
            pack_bytes: self.pack_bytes.saturating_sub(before.pack_bytes),
            branch_requests: self.branch_requests.saturating_sub(before.branch_requests),
            branch_chain_edges: self
                .branch_chain_edges
                .saturating_sub(before.branch_chain_edges),
            branch_delta_applies: self
                .branch_delta_applies
                .saturating_sub(before.branch_delta_applies),
            page_requests: self.page_requests.saturating_sub(before.page_requests),
            page_chain_edges: self
                .page_chain_edges
                .saturating_sub(before.page_chain_edges),
            page_delta_applies: self
                .page_delta_applies
                .saturating_sub(before.page_delta_applies),
            program_chain_canonical: self
                .program_chain_canonical
                .saturating_sub(before.program_chain_canonical),
            program_chain_encoded: self
                .program_chain_encoded
                .saturating_sub(before.program_chain_encoded),
            // A maximum has no difference; the later reading is the bound.
            program_max_depth: self.program_max_depth,
        }
    }

    /// Adds another reader or wave without wrapping lifetime totals.
    pub fn accumulate(&mut self, other: Self) {
        self.leaf_requests = self.leaf_requests.saturating_add(other.leaf_requests);
        self.chain_edges = self.chain_edges.saturating_add(other.chain_edges);
        self.physical_record_calls = self
            .physical_record_calls
            .saturating_add(other.physical_record_calls);
        self.physical_group_decodes = self
            .physical_group_decodes
            .saturating_add(other.physical_group_decodes);
        self.physical_group_decoded_bytes = self
            .physical_group_decoded_bytes
            .saturating_add(other.physical_group_decoded_bytes);
        self.physical_group_cache_hits = self
            .physical_group_cache_hits
            .saturating_add(other.physical_group_cache_hits);
        self.value_lookups = self.value_lookups.saturating_add(other.value_lookups);
        self.value_cache_hits = self.value_cache_hits.saturating_add(other.value_cache_hits);
        self.value_cache_evictions = self
            .value_cache_evictions
            .saturating_add(other.value_cache_evictions);
        self.value_rows = self.value_rows.saturating_add(other.value_rows);
        self.pack_fetches = self.pack_fetches.saturating_add(other.pack_fetches);
        self.pack_bytes = self.pack_bytes.saturating_add(other.pack_bytes);
        self.branch_requests = self.branch_requests.saturating_add(other.branch_requests);
        self.branch_chain_edges = self
            .branch_chain_edges
            .saturating_add(other.branch_chain_edges);
        self.branch_delta_applies = self
            .branch_delta_applies
            .saturating_add(other.branch_delta_applies);
        self.page_requests = self.page_requests.saturating_add(other.page_requests);
        self.page_chain_edges = self.page_chain_edges.saturating_add(other.page_chain_edges);
        self.page_delta_applies = self
            .page_delta_applies
            .saturating_add(other.page_delta_applies);
        self.program_chain_canonical = self
            .program_chain_canonical
            .saturating_add(other.program_chain_canonical);
        self.program_chain_encoded = self
            .program_chain_encoded
            .saturating_add(other.program_chain_encoded);
        self.program_max_depth = self.program_max_depth.max(other.program_max_depth);
    }
}

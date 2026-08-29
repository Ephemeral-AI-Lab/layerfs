#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlacementAnalysis {
    pub role: String,
    pub object_count: u64,
    pub encoded_bytes: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DedupAnalysis {
    pub route_cas_bytes: u64,
    pub union_cas_bytes: u64,
    pub cross_store_placement_bytes: u64,
    pub placement_factor: f64,
    pub placements: Vec<PlacementAnalysis>,
}

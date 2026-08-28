use super::{BranchHead, LayerStackHead, TransferReceipt};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FetchBranchReceipt {
    pub head: BranchHead,
    pub origin_stack_head: LayerStackHead,
    pub transfer: TransferReceipt,
    pub dependency_transfer: Option<TransferReceipt>,
    pub history_export_ns: u128,
    pub closure_traversal_ns: u128,
    pub head_transaction_ns: u128,
    pub complete_wall_ns: u128,
    pub terminal_object_page_entries: u64,
    pub pages: u64,
    pub dependency_pages: u64,
    pub complete: bool,
}

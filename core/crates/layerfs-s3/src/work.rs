//! Request-class wall observations; concurrent request walls may overlap.
/// Aggregate work for PUT, GET or HEAD, in that order in the diagnostics array.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct S3RequestWork {
    /// Attempted requests of this method.
    pub calls: u64,
    /// Request signing wall before connection selection.
    pub signing_ns: u64,
    /// Conditional body validation wall before request submission.
    pub validation_ns: u64,
    /// Header submission wall.
    pub header_write_ns: u64,
    /// Wait for interim or early-final PUT headers.
    pub continue_wait_ns: u64,
    /// Body submission wall.
    pub body_write_ns: u64,
    /// Wait for final headers.
    pub response_head_ns: u64,
    /// Entity/framing response wall.
    pub response_body_ns: u64,
    /// Actual submitted body bytes.
    pub body_sent: u64,
    /// Actual received entity bytes.
    pub body_received: u64,
}
impl S3RequestWork {
    pub(crate) fn accumulate(&mut self, other: Self) {
        self.calls += other.calls;
        self.signing_ns += other.signing_ns;
        self.validation_ns += other.validation_ns;
        self.header_write_ns += other.header_write_ns;
        self.continue_wait_ns += other.continue_wait_ns;
        self.body_write_ns += other.body_write_ns;
        self.response_head_ns += other.response_head_ns;
        self.response_body_ns += other.response_body_ns;
        self.body_sent += other.body_sent;
        self.body_received += other.body_received;
    }
}
pub(crate) fn index(method: &str) -> usize {
    match method {
        "PUT" => 0,
        "GET" => 1,
        _ => 2,
    }
}

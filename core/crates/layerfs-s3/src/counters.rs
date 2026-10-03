//! Actual transport counts, never performance measurements.
/// Cumulative counts for one persistent client.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct S3Diagnostics {
    /// TCP connect and socket configuration wall.
    pub connect_ns: u64,
    /// Request header submission wall.
    pub header_write_ns: u64,
    /// Wait for the conditional PUT interim or early final response.
    pub continue_wait_ns: u64,
    /// Request body submission wall.
    pub body_write_ns: u64,
    /// Wait for final response headers.
    pub response_head_ns: u64,
    /// Response framing and entity read wall.
    pub response_body_ns: u64,
    /// Per-method work: PUT, GET, HEAD. Request walls may overlap.
    pub request_work: [crate::S3RequestWork; 3],
    /// Successfully opened TCP connections.
    pub connections: u64,
    /// Attempted HTTP requests.
    pub requests: u64,
    /// HTTP 100 responses within a single conditional PUT attempt.
    pub interim_responses: u64,
    /// PUT requests.
    pub puts: u64,
    /// GET requests.
    pub gets: u64,
    /// HEAD requests.
    pub heads: u64,
    /// Bytes written to the socket, including headers.
    pub wire_sent: u64,
    /// Bytes read from the socket, including headers/framing.
    pub wire_received: u64,
    /// Request body bytes written to the socket.
    pub body_sent: u64,
    /// Response entity bytes consumed, including refusal bodies.
    pub body_received: u64,
}

impl S3Diagnostics {
    pub(crate) fn accumulate(&mut self, other: Self) {
        for (work, incoming) in self.request_work.iter_mut().zip(other.request_work) {
            work.accumulate(incoming);
        }
        self.connections += other.connections;
        self.connect_ns += other.connect_ns;
        self.header_write_ns += other.header_write_ns;
        self.continue_wait_ns += other.continue_wait_ns;
        self.body_write_ns += other.body_write_ns;
        self.response_head_ns += other.response_head_ns;
        self.response_body_ns += other.response_body_ns;

        self.requests += other.requests;
        self.interim_responses += other.interim_responses;
        self.puts += other.puts;
        self.gets += other.gets;
        self.heads += other.heads;
        self.wire_sent += other.wire_sent;
        self.wire_received += other.wire_received;
        self.body_sent += other.body_sent;
        self.body_received += other.body_received;
    }
}

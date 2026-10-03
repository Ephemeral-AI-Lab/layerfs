//! Actual transport counts, never performance measurements.
/// Cumulative counts for one persistent client.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct S3Diagnostics {
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
        self.connections += other.connections;
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

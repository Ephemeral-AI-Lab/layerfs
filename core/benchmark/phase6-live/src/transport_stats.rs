//! Fixed-size observations of actual native metadata operations.
#[derive(Clone, Copy, Debug, Default)]
pub struct Statistics {
    pub calls: [u64; 7],
    pub connect_attempts: u64,
    pub connect_ns: u64,
    pub request_ns: [u64; 7],
}
impl Statistics {
    pub fn json(self) -> String {
        format!(
            "{{\"calls\":{:?},\"connect_attempts\":{},\"connect_ns\":{},\"request_ns\":{:?}}}",
            self.calls, self.connect_attempts, self.connect_ns, self.request_ns
        )
    }
}

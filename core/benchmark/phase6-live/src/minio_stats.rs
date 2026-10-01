//! Fixed-size actual request/body observations; no lifetime request registry.
#[derive(Clone, Copy, Debug, Default)]
pub struct Statistics {
    pub put_calls: u64,
    pub get_calls: u64,
    pub put_requested_bytes: u64,
    pub get_received_bytes: u64,
}
impl Statistics {
    pub fn json(self) -> String {
        format!("{{\"put_calls\":{},\"get_calls\":{},\"put_requested_bytes\":{},\"get_received_bytes\":{}}}",self.put_calls,self.get_calls,self.put_requested_bytes,self.get_received_bytes)
    }
}

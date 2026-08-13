#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FaultPoint {
    calls: u64,
    target: Option<u64>,
    current: Option<u64>,
}

impl Default for FaultPoint {
    fn default() -> Self {
        Self {
            calls: 0,
            target: None,
            current: None,
        }
    }
}

impl FaultPoint {
    pub const fn cancel_at(target: u64) -> Self {
        Self {
            calls: 0,
            target: Some(target),
            current: None,
        }
    }

    pub fn calls(&self) -> u64 {
        self.calls
    }
}

impl FaultPoint {
    pub fn observe(&mut self, boundary: u64) -> bool {
        self.calls += 1;
        self.current = Some(boundary);
        self.current == self.target
    }
}

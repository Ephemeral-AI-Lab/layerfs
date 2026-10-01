//! One admitted native metadata session; no request is resent after uncertainty.
use crate::{transport_stats::Statistics, wire};
use layerfs_bridge::adapters::native::{
    connection::{self, Connection},
    protocol::{Frame, Kind},
};
use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};
const IDLE_LIMIT: Duration = Duration::from_secs(2);
#[derive(Default)]
pub struct Session {
    connection: Option<Connection>,
    idle_since: Option<Instant>,
    next: u64,
    quarantined: bool,
    pub statistics: Statistics,
}
impl Session {
    pub fn call(
        &mut self,
        address: SocketAddr,
        selector: u32,
        private: &[u8; 32],
        server: &[u8; 32],
        action: u8,
        payload: &[u8],
    ) -> Result<Vec<u8>, String> {
        if payload.len().checked_add(8).is_none_or(|n| n > 16384) || action > 7 {
            return Err("metadata request capacity/action".into());
        }
        if self.quarantined {
            return Err("metadata session quarantined; no resend".into());
        }
        if self
            .idle_since
            .is_some_and(|last| last.elapsed() > IDLE_LIMIT)
        {
            self.close_idle();
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        if self.connection.is_none() {
            let started = Instant::now();
            let result = connection::connect_until(address, selector, private, server, deadline)
                .map_err(|e| e.to_string());
            self.statistics.connect_attempts += 1;
            self.statistics.connect_ns = self
                .statistics
                .connect_ns
                .saturating_add(started.elapsed().as_nanos() as u64);
            match result {
                Ok(c) => self.connection = Some(c),
                Err(e) => {
                    self.quarantined = true;
                    return Err(e);
                }
            }
        }
        self.next = self
            .next
            .checked_add(1)
            .ok_or("metadata request ID exhaustion")?;
        let id = self.next;
        let mut bytes = wire::PREFIX.to_vec();
        bytes.push(action);
        bytes.extend_from_slice(payload);
        let started = Instant::now();
        let connection = self.connection.as_mut().ok_or("metadata session missing")?;
        connection.receive.deadline(deadline);
        connection.send.deadline(deadline);
        let result = (|| {
            connection
                .send
                .write(&Frame {
                    kind: Kind::Begin,
                    id,
                    bytes,
                })
                .map_err(|e| e.to_string())?;
            let reply = connection.receive.read().map_err(|e| e.to_string())?;
            if reply.id != id
                || reply.kind != Kind::Success
                || !reply.bytes.starts_with(wire::PREFIX)
            {
                return Err("metadata failure/profile/reply identity".into());
            }
            Ok(reply.bytes[wire::PREFIX.len()..].to_vec())
        })();
        self.statistics.calls[action as usize] += 1;
        self.statistics.request_ns[action as usize] = self.statistics.request_ns[action as usize]
            .saturating_add(started.elapsed().as_nanos() as u64);
        match result {
            Ok(bytes) => {
                self.idle_since = Some(Instant::now());
                Ok(bytes)
            }
            Err(e) => {
                self.quarantined = true;
                self.close_idle();
                Err(e)
            }
        }
    }
    fn close_idle(&mut self) {
        if let Some(c) = self.connection.take() {
            c.send.close();
            c.receive.close();
        }
        self.idle_since = None;
    }
}

//! Actual daemon stdout startup event; bounded line and multiplexed payload windows.
use super::{body::exact, Sandbox};
use super::{body::Body, socket::Socket};
use crate::{RuntimeError, WireFailure};
use std::{
    io::{self, BufReader, Read},
    net::Shutdown,
    time::{Duration, Instant},
};
/// Startup bytes observed before the exact complete stdout marker, never a FUSE Ready claim.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ListenerObservation {
    pub stdout: u64,
    pub stderr: u64,
    pub marker_seen: bool,
}
/// Original startup stream progress and transfer/fence failure.
#[derive(Debug)]
pub struct ListenerFailure {
    pub observation: ListenerObservation,
    pub transfer: Box<WireFailure>,
}
impl Sandbox {
    /// Waits once for the actual listener marker from this acknowledged start's container.
    /// Uses explicit control I/O waits; it never imposes a deadline on ordinary commands.
    pub fn wait_listener(
        &mut self,
        timeout: Duration,
    ) -> Result<ListenerObservation, Box<ListenerFailure>> {
        let mut observation = ListenerObservation::default();
        let result = (|| {
            if !self.started
                || self.listen_attempted
                || self.stop_attempted
                || self.delete_attempted
            {
                return Err(self.invalid("original listener wait admission"));
            }
            let deadline = Instant::now()
                .checked_add(timeout)
                .filter(|_| !timeout.is_zero())
                .ok_or_else(|| self.invalid("explicit startup wait"))?;
            self.listen_attempted = true;
            let mut pending = self.docker.clone();
            pending.deadline = Some(deadline);
            let mut r = pending.exchange(
                    "GET",
                    &format!(
                        "/v1.54/containers/{}/logs?stdout=true&stderr=true&follow=true&tail=all&timestamps=false",
                        self.id
                    ),
                    false,
                    |_| Ok(()),
                )
                .map_err(|f| self.selected(f))?;
            if r.status != 200 {
                return Err(self.selected(r.fail(RuntimeError::Http(r.status), None)));
            }
            if !r.multiplexed {
                return Err(self.selected(r.fail(
                    RuntimeError::Protocol("daemon logs multiplexed media"),
                    None,
                )));
            }
            let expected = format!("LAYERFS_DAEMON_LISTEN 0.0.0.0:{}", self.request.port);
            let parsed = (|| {
                let mut input = StartupRead {
                    body: &mut r.body,
                    deadline,
                    wait: self.docker.control_wait,
                };
                let mut row = [0; 256];
                let mut used = 0;
                let mut overflow = false;
                let mut payload = [0; 8192];
                loop {
                    let mut header = [0; 8];
                    exact(&mut input, &mut header)?;
                    let (tag, mut left) = super::mux::header(header)?;
                    if !matches!(tag, 1 | 2) {
                        return Err(RuntimeError::Protocol("daemon log frame"));
                    }
                    while left != 0 {
                        let wanted = (left as usize).min(payload.len());
                        let n = input.read(&mut payload[..wanted])?;
                        if n == 0 {
                            return Err(
                                std::io::Error::from(std::io::ErrorKind::UnexpectedEof).into()
                            );
                        }
                        left -= n as u32;
                        if tag == 2 {
                            observation.stderr = observation.stderr.saturating_add(n as u64);
                            continue;
                        }
                        observation.stdout = observation.stdout.saturating_add(n as u64);
                        for &byte in &payload[..n] {
                            if byte == b'\n' {
                                if !overflow && row[..used] == *expected.as_bytes() {
                                    observation.marker_seen = true;
                                }
                                used = 0;
                                overflow = false;
                            } else if used < row.len() {
                                row[used] = byte;
                                used += 1;
                            } else {
                                overflow = true;
                            }
                        }
                    }
                    if observation.marker_seen {
                        return Ok(());
                    }
                }
            })();
            parsed.map_err(|e| self.selected(r.fail(e, None)))?;
            // This is the original logs fence, once. A failed fence keeps marker evidence separate.
            r.writer
                .stream
                .shutdown(Shutdown::Both)
                .map_err(|error| self.selected(r.failed_fence(error)))?;
            Ok(())
        })();
        result.map(|()| observation).map_err(|transfer| {
            Box::new(ListenerFailure {
                observation,
                transfer,
            })
        })
    }
}

/// Absolute startup wait checked before every body read, including fragmented mux headers.
struct StartupRead<'a> {
    body: &'a mut Body<BufReader<Socket>>,
    deadline: Instant,
    wait: Duration,
}
impl Read for StartupRead<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let left = self
            .deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| io::Error::from(io::ErrorKind::TimedOut))?;
        self.body.inner_mut().get_mut().wait = Some(left.min(self.wait));
        self.body.read(out)
    }
}

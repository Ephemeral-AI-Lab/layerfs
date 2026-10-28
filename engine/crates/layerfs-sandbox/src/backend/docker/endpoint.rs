//! Structurally observed exact container and loopback-published daemon endpoint.
use super::{
    json::{once, Json},
    Sandbox,
};
use crate::{ContainerId, RuntimeError, WireFailure};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
impl Sandbox {
    /// Reopens an exact saved container only after validating immutable deployment and topology.
    /// This performs no create, upload, start, installation or command replay.
    pub fn reopen(
        docker: super::Docker,
        id: ContainerId,
        request: super::SandboxRequest,
    ) -> Result<Self, Box<WireFailure>> {
        let owner = Self {
            docker,
            id,
            request,
            upload_attempted: true,
            uploaded: true,
            start_attempted: true,
            started: true,
            listen_attempted: true,
            stop_attempted: false,
            stopped: false,
            delete_attempted: false,
            deleted: false,
        };
        owner
            .request
            .check()
            .map_err(|_| owner.invalid("saved deployment"))?;
        owner.endpoint()?;
        Ok(owner)
    }

    /// An exact saved container that [`Self::reopen`] observed stopped
    /// ([`WireFailure::container_stopped`]): only its one Delete is admitted.
    /// The Engine refuses that Delete if the container runs again meanwhile.
    pub fn stopped(docker: super::Docker, id: ContainerId, request: super::SandboxRequest) -> Self {
        Self {
            docker,
            id,
            request,
            upload_attempted: true,
            uploaded: true,
            start_attempted: true,
            started: true,
            listen_attempted: true,
            stop_attempted: true,
            stopped: true,
            delete_attempted: false,
            deleted: false,
        }
    }

    /// Observes one exact running container and its single expected loopback port mapping.
    /// Neither port allocation nor Running is filesystem Ready.
    pub fn endpoint(&self) -> Result<SocketAddr, Box<WireFailure>> {
        if !self.started || self.stop_attempted || self.delete_attempted {
            return Err(self.invalid("running endpoint admission"));
        }
        let (_, endpoint) = self.observe(true)?;
        endpoint.ok_or_else(|| self.invalid("missing daemon endpoint"))
    }
    /// One exact deployment/state observation; absent optional facts remain absent.
    pub fn inspect(&self) -> Result<crate::provider::SandboxInspection, Box<WireFailure>> {
        self.observe(false).map(|(inspection, _)| inspection)
    }
    fn observe(
        &self,
        require_running: bool,
    ) -> Result<(crate::provider::SandboxInspection, Option<SocketAddr>), Box<WireFailure>> {
        if self.deleted {
            return Err(self.invalid("deleted container observation"));
        }
        let mut r = self
            .docker
            .exchange(
                "GET",
                &format!("/containers/{}/json", self.id),
                false,
                |_| Ok(()),
            )
            .map_err(|f| self.selected(f))?;
        if r.status != 200 {
            return Err(self.selected(r.fail(RuntimeError::Http(r.status), None)));
        }
        let mut observed = None;
        let result = (|| {
            let mut fields = 0;
            let mut image = false;
            let mut topology = true;
            let mut running = None;
            let mut endpoint = None;
            let mut exit_code = None;
            let mut process_id = None;
            let mut j = Json::new(&mut r.body);
            j.object(|j, key| {
                if key.equals("Id") {
                    once(&mut fields, 1)?;
                    let id = j.string(true)?;
                    if id.overflow {
                        return Err(RuntimeError::Protocol("container inspect ID width"));
                    }
                    observed = Some(ContainerId::parse(id.text())?);
                } else if key.equals("Image") {
                    once(&mut fields, 8)?;
                    image = j.string_equals(&self.request.image)?;
                } else if key.equals("Config") {
                    once(&mut fields, 16)?;
                    topology &= super::topology::config(j, &self.request.labels)?;
                } else if key.equals("HostConfig") {
                    once(&mut fields, 32)?;
                    topology &= super::topology::host(j)?;
                } else if key.equals("Mounts") {
                    once(&mut fields, 64)?;
                    topology &= super::topology::mounts(j, &self.request)?;
                } else if key.equals("State") {
                    once(&mut fields, 2)?;
                    let mut bits = 0;
                    j.object(|j, key| {
                        if key.equals("Running") {
                            once(&mut bits, 1)?;
                            running = Some(j.boolean()?);
                        } else if key.equals("ExitCode") {
                            once(&mut bits, 16)?;
                            if !j.null()? {
                                exit_code =
                                    Some(i32::try_from(j.integer()?).map_err(|_| {
                                        RuntimeError::Protocol("container exit width")
                                    })?);
                            }
                        } else if key.equals("Pid") {
                            once(&mut bits, 32)?;
                            process_id = Some(
                                u32::try_from(j.integer()?)
                                    .map_err(|_| RuntimeError::Protocol("container PID width"))?,
                            );
                        } else if key.equals("Paused")
                            || key.equals("Restarting")
                            || key.equals("Dead")
                        {
                            let bit = if key.equals("Paused") {
                                2
                            } else if key.equals("Restarting") {
                                4
                            } else {
                                8
                            };
                            once(&mut bits, bit)?;
                            topology &= !j.boolean()?;
                        } else {
                            j.skip(2)?;
                        }
                        Ok(())
                    })?;
                    topology &= bits & 15 == 15;
                } else if key.equals("NetworkSettings") {
                    once(&mut fields, 4)?;
                    let mut bits = 0;
                    j.object(|j, key| {
                        if key.equals("Ports") {
                            once(&mut bits, 1)?;
                            let mut selected = 0;
                            j.object(|j, key| {
                                if key.equals(&format!("{}/tcp", self.request.port)) {
                                    once(&mut selected, 1)?;
                                    if j.null()? {
                                        return Ok(());
                                    }
                                    let mut count = 0;
                                    j.array(|j| {
                                        count += 1;
                                        if count != 1 {
                                            return Err(RuntimeError::Protocol(
                                                "single daemon endpoint",
                                            ));
                                        }
                                        let mut bits = 0;
                                        let mut ip = None;
                                        let mut port = None;
                                        j.object(|j, key| {
                                            if key.equals("HostIp") {
                                                once(&mut bits, 1)?;
                                                let v = j.string(true)?;
                                                if v.overflow {
                                                    return Err(RuntimeError::Protocol(
                                                        "endpoint address width",
                                                    ));
                                                }
                                                ip = Some(v.text().parse::<IpAddr>().map_err(
                                                    |_| RuntimeError::Protocol("endpoint address"),
                                                )?);
                                            } else if key.equals("HostPort") {
                                                once(&mut bits, 2)?;
                                                let v = j.string(true)?;
                                                if v.overflow
                                                    || v.text().is_empty()
                                                    || !v.text().bytes().all(|b| b.is_ascii_digit())
                                                {
                                                    return Err(RuntimeError::Protocol(
                                                        "endpoint port",
                                                    ));
                                                }
                                                port = Some(v.text().parse::<u16>().map_err(
                                                    |_| {
                                                        RuntimeError::Protocol(
                                                            "endpoint port width",
                                                        )
                                                    },
                                                )?);
                                            } else {
                                                j.skip(5)?;
                                            }
                                            Ok(())
                                        })?;
                                        if ip != Some(IpAddr::V4(Ipv4Addr::LOCALHOST))
                                            || port.is_none()
                                            || port == Some(0)
                                        {
                                            return Err(RuntimeError::Protocol(
                                                "exact loopback endpoint",
                                            ));
                                        }
                                        endpoint = Some(SocketAddr::new(
                                            ip.expect("checked"),
                                            port.expect("checked"),
                                        ));
                                        Ok(())
                                    })?;
                                } else {
                                    j.skip(4)?;
                                }
                                Ok(())
                            })?;
                        } else {
                            j.skip(2)?;
                        }
                        Ok(())
                    })?;
                } else {
                    j.skip(1)?;
                }
                Ok(())
            })?;
            j.finish()?;
            if observed != Some(self.id)
                || fields != 127
                || !image
                || !topology
                || running.is_none()
            {
                return Err(RuntimeError::Protocol(
                    "exact running container observation",
                ));
            }
            if require_running && running == Some(false) {
                return Err(RuntimeError::Protocol(crate::types::STOPPED));
            }
            if running == Some(true) && endpoint.is_none() {
                return Err(RuntimeError::Protocol("missing daemon endpoint"));
            }
            Ok((
                crate::provider::SandboxInspection {
                    identity: self.id,
                    running: running.expect("required"),
                    exit_code: if running == Some(false) {
                        exit_code
                    } else {
                        None
                    },
                    process_id: process_id.filter(|pid| *pid != 0),
                    scratch_bytes: None,
                },
                endpoint,
            ))
        })();
        result.map_err(|e| {
            let mut f = self.selected(r.fail(e, None));
            f.observed_container = observed;
            f
        })
    }
}

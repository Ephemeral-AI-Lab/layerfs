//! One-attempt create/start/stop/delete, with no implicit adoption or volume deletion.
use super::{
    container_types::{CreatePhase, Sandbox, SandboxCreateFailure, SandboxRequest},
    json::{once, Json},
    strings::quoted,
    Docker,
};
use crate::{ContainerId, RuntimeError, WireFailure};
use std::io::{self, Read, Write};
impl SandboxRequest {
    /// Native mounting needs exactly one capability and one device. The
    /// container stays unprivileged with no-new-privileges; commands run as
    /// their own nonroot identity and inherit neither.
    fn encode(&self, w: &mut dyn Write) -> io::Result<()> {
        w.write_all(b"{\"Image\":")?;
        quoted(w, &self.image)?;
        w.write_all(b",\"User\":\"0:0\",\"Entrypoint\":[\"/usr/local/bin/layerfs-daemon\"],\"Cmd\":[\"--config\",\"/layerfs-local/config/daemon.setup\"],\"Tty\":false,\"ExposedPorts\":{")?;
        quoted(w, &format!("{}/tcp", self.port))?;
        w.write_all(b":{}},\"HostConfig\":{\"RestartPolicy\":{\"Name\":\"no\"},\"LogConfig\":{\"Type\":\"json-file\"},\"Privileged\":false,\"CapAdd\":[\"CAP_SYS_ADMIN\"],\"Devices\":[{\"PathOnHost\":\"/dev/fuse\",\"PathInContainer\":\"/dev/fuse\",\"CgroupPermissions\":\"rwm\"}],\"SecurityOpt\":[\"no-new-privileges=true\",\"apparmor=unconfined\"],\"Mounts\":[{\"Type\":\"volume\",\"Source\":")?;
        quoted(w, &self.store_volume)?;
        w.write_all(b",\"Target\":\"/layerfs-store\",\"ReadOnly\":false,\"VolumeOptions\":{\"NoCopy\":true}}],\"PortBindings\":{")?;
        quoted(w, &format!("{}/tcp", self.port))?;
        w.write_all(b":[{\"HostIp\":\"127.0.0.1\",\"HostPort\":\"\"}]}}}")
    }
}
impl Docker {
    /// Validates the borrowed volume once, then creates one actual stopped container.
    pub fn create_sandbox(
        &self,
        request: SandboxRequest,
    ) -> Result<Sandbox, Box<SandboxCreateFailure>> {
        let mut phase = CreatePhase::Validate;
        let result = (|| {
            request
                .check()
                .map_err(|e| super::http::failure(false, 0, None, e, None))?;
            phase = CreatePhase::Volume;
            let mut response = self.exchange(
                "GET",
                &format!("/v1.54/volumes/{}", request.store_volume),
                false,
                |_| Ok(()),
            )?;
            if response.status != 200 {
                return Err(response.fail(RuntimeError::Http(response.status), None));
            }
            let result = (|| {
                let mut found = false;
                let mut bits = 0;
                let mut local = false;
                let mut scope = false;
                let mut options = false;
                let mut j = Json::new(&mut response.body);
                j.object(|j, key| {
                    if key.equals("Name") {
                        once(&mut bits, 1)?;
                        found = j.string_equals(&request.store_volume)?;
                    } else if key.equals("Driver") {
                        once(&mut bits, 2)?;
                        local = j.string_equals("local")?;
                    } else if key.equals("Scope") {
                        once(&mut bits, 4)?;
                        scope = j.string_equals("local")?;
                    } else if key.equals("Options") {
                        once(&mut bits, 8)?;
                        options = true;
                        if !j.null()? {
                            j.object(|j, _| {
                                options = false;
                                j.skip(2)
                            })?;
                        }
                    } else {
                        j.skip(1)?;
                    }
                    Ok(())
                })?;
                j.finish()?;
                if bits != 15 || !found || !local || !scope || !options {
                    return Err(RuntimeError::Protocol("exact plain local VM volume"));
                }
                Ok(())
            })();
            result.map_err(|e| response.fail(e, None))?;
            phase = CreatePhase::Create;
            let mut response = self.exchange("POST", "/v1.54/containers/create", false, |w| {
                request.encode(w)
            })?;
            if response.status != 201 {
                return Err(response.fail(RuntimeError::Http(response.status), None));
            }
            let mut observed = None;
            let result = (|| {
                let mut bits = 0;
                let mut j = Json::new(&mut response.body);
                j.object(|j, key| {
                    if key.equals("Id") {
                        once(&mut bits, 1)?;
                        let id = j.string(true)?;
                        if id.overflow {
                            return Err(RuntimeError::Protocol("container ID width"));
                        }
                        observed = Some(ContainerId::parse(id.text())?);
                    } else {
                        j.skip(1)?;
                    }
                    Ok(())
                })?;
                j.finish()?;
                observed.ok_or(RuntimeError::Protocol("missing container ID"))
            })();
            result.map_err(|e| {
                let mut f = response.fail(e, None);
                f.observed_container = observed;
                f
            })
        })();
        match result {
            Ok(id) => Ok(Sandbox {
                docker: self.clone(),
                id,
                request,
                upload_attempted: false,
                uploaded: false,
                start_attempted: false,
                started: false,
                listen_attempted: false,
                stop_attempted: false,
                stopped: false,
                delete_attempted: false,
                deleted: false,
            }),
            Err(transfer) => Err(Box::new(SandboxCreateFailure {
                request,
                phase,
                transfer,
            })),
        }
    }
}
impl Sandbox {
    /// Creates an ordinary command against this exact known running Sandbox, once.
    pub fn create_exec(
        &self,
        request: super::ExecRequest,
    ) -> Result<super::ExecCreated, Box<super::ExecCreateFailure>> {
        if !self.started || self.stop_attempted || self.delete_attempted {
            return Err(Box::new(super::ExecCreateFailure {
                container: self.id,
                request,
                transfer: self.invalid("running command admission"),
            }));
        }
        self.docker.create_exec(self.id, request)
    }

    /// Starts once after acknowledged private upload; no command or filesystem drain is inferred.
    pub fn start(&mut self) -> Result<(), Box<WireFailure>> {
        if !self.uploaded || self.start_attempted || self.delete_attempted || self.stop_attempted {
            return Err(self.invalid("original container Start admission"));
        }
        self.start_attempted = true;
        self.empty("POST", &format!("/v1.54/containers/{}/start", self.id), 204)?;
        self.started = true;
        Ok(())
    }
    /// Explicit whole-Sandbox process stop, once. This is crash teardown, not graceful FS drain.
    pub fn stop(&mut self, seconds: u16) -> Result<(), Box<WireFailure>> {
        if self.stop_attempted || self.delete_attempted {
            return Err(self.invalid("original container Stop admission"));
        }
        self.stop_attempted = true;
        self.empty(
            "POST",
            &format!("/v1.54/containers/{}/stop?t={seconds}", self.id),
            204,
        )?;
        self.stopped = true;
        Ok(())
    }
    /// Deletes the exact acknowledged container/local writable layer once. Shared Store volume is borrowed.
    pub fn delete(&mut self) -> Result<(), Box<WireFailure>> {
        if self.delete_attempted || (self.start_attempted && !self.stopped) {
            return Err(self.invalid("original container Delete admission; known stop required"));
        }
        self.delete_attempted = true;
        self.empty(
            "DELETE",
            &format!("/v1.54/containers/{}?force=false&v=false", self.id),
            204,
        )?;
        self.deleted = true;
        Ok(())
    }
    fn empty(&self, method: &str, path: &str, status: u16) -> Result<(), Box<WireFailure>> {
        let mut response = self
            .docker
            .exchange(method, path, false, |_| Ok(()))
            .map_err(|f| self.selected(f))?;
        if response.status != status {
            return Err(self.selected(response.fail(RuntimeError::Http(response.status), None)));
        }
        let mut byte = [0];
        match response.body.read(&mut byte) {
            Ok(0) => Ok(()),
            Ok(_) => Err(self.selected(response.fail(
                RuntimeError::Protocol("unexpected lifecycle response body"),
                None,
            ))),
            Err(e) => Err(self.selected(response.fail(e.into(), None))),
        }
    }
}

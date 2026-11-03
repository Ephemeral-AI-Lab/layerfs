//! One-attempt create and start, with no implicit adoption or volume deletion.
use super::{
    sandbox::{
        store_source, CreatePhase, Sandbox, SandboxCreateFailure, SandboxRequest, StoreSource,
        DESKTOP_HOST,
    },
    topology::SECCOMP,
    Docker,
};
use crate::json::quoted;
use crate::json::{once, Json};
use crate::{ContainerId, RuntimeError, WireFailure};
use std::io::{self, Write};
impl SandboxRequest {
    /// Kernel mounting needs exactly one capability and one device. The
    /// container stays unprivileged with no-new-privileges; commands run as
    /// their own nonroot identity and inherit neither.
    /// Nothing in it may create a user namespace: see [`SECCOMP`].
    /// The Engine's init is PID 1 and the daemon its child: a process a
    /// command leaves behind is reaped when it exits instead of staying a
    /// zombie, and the daemon's own waits keep their children's statuses.
    /// On Linux `host.docker.internal` is added as the Engine's
    /// `host-gateway`, as Docker Desktop already names it.
    fn encode(&self, w: &mut dyn Write) -> io::Result<()> {
        w.write_all(b"{\"Image\":")?;
        quoted(w, &self.image)?;
        w.write_all(b",\"Labels\":{")?;
        for (i, (key, value)) in self.labels.iter().enumerate() {
            if i > 0 {
                w.write_all(b",")?;
            }
            quoted(w, key)?;
            w.write_all(b":")?;
            quoted(w, value)?;
        }
        w.write_all(b"},\"User\":\"0:0\",\"Entrypoint\":[\"/usr/local/bin/layerfs-daemon\"],\"Cmd\":[\"--config\",\"/layerfs-local/config/daemon.setup\"],\"Tty\":false,\"ExposedPorts\":{")?;
        quoted(w, &format!("{}/tcp", self.port))?;
        w.write_all(b":{}},\"HostConfig\":{\"Init\":true,\"RestartPolicy\":{\"Name\":\"no\"},\"LogConfig\":{\"Type\":\"json-file\",\"Config\":{\"max-size\":\"10m\",\"max-file\":\"2\"}},\"Privileged\":false,\"CapAdd\":[\"CAP_SYS_ADMIN\"],\"Devices\":[{\"PathOnHost\":\"/dev/fuse\",\"PathInContainer\":\"/dev/fuse\",\"CgroupPermissions\":\"rwm\"}],\"SecurityOpt\":[\"no-new-privileges=true\",\"apparmor=unconfined\",")?;
        quoted(w, SECCOMP)?;
        w.write_all(b"],")?;
        if cfg!(target_os = "linux") {
            // a Linux Engine has no `host.docker.internal` of its own: the
            // name `reach` gives a loopback PostgreSQL server (#535 §7.4)
            // resolves to the bridge's gateway, the host
            w.write_all(b"\"ExtraHosts\":[")?;
            quoted(w, &format!("{DESKTOP_HOST}:host-gateway"))?;
            w.write_all(b"],")?;
        }
        w.write_all(b"\"Mounts\":[")?;
        match self.store_volume.as_deref().and_then(store_source) {
            Some(StoreSource::Volume { name, subpath }) => {
                w.write_all(b"{\"Type\":\"volume\",\"Source\":")?;
                quoted(w, name)?;
                w.write_all(b",\"Target\":\"/layerfs-store\",\"ReadOnly\":false,\"VolumeOptions\":{\"NoCopy\":true")?;
                if let Some(path) = subpath {
                    w.write_all(b",\"Subpath\":")?;
                    quoted(w, path)?;
                }
                w.write_all(b"}}")?;
            }
            Some(StoreSource::Bind(path)) => {
                w.write_all(b"{\"Type\":\"bind\",\"Source\":")?;
                quoted(w, path)?;
                w.write_all(b",\"Target\":\"/layerfs-store\",\"ReadOnly\":false}")?;
            }
            None => {}
        }
        w.write_all(b"],\"PortBindings\":{")?;
        quoted(w, &format!("{}/tcp", self.port))?;
        w.write_all(b":[{\"HostIp\":\"127.0.0.1\",\"HostPort\":\"\"}]}}}")
    }
}
impl Docker {
    /// One read of an existing exact plain local volume; it has no create effect.
    fn plain_volume(&self, name: &str) -> Result<(), Box<WireFailure>> {
        let mut response = self.exchange("GET", &format!("/volumes/{name}"), false, |_| Ok(()))?;
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
                    found = j.string_equals(name)?;
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
        Ok(())
    }
    /// Reads the borrowed Store volume once, then creates one actual stopped container.
    pub fn create_sandbox(
        &self,
        request: SandboxRequest,
    ) -> Result<Sandbox, Box<SandboxCreateFailure>> {
        let mut phase = CreatePhase::Validate;
        let result = (|| {
            request
                .check()
                .map_err(|e| super::client::http::failure(false, 0, None, e, None))?;
            phase = CreatePhase::Volume;
            if let Some(StoreSource::Volume { name, .. }) =
                request.store_volume.as_deref().and_then(store_source)
            {
                self.plain_volume(name)?;
            }
            phase = CreatePhase::Create;
            // `naming` admitted only characters that need no URL escape.
            let path = match &self.container_name {
                Some(name) => format!("/containers/create?name={name}"),
                None => "/containers/create".to_owned(),
            };
            let mut response = self.exchange("POST", &path, false, |w| request.encode(w))?;
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
        self.empty("POST", &format!("/containers/{}/start", self.id), 204)?;
        self.started = true;
        Ok(())
    }
}

//! One machine's own checks (call surface 2.0, `calls.json`
//! `doctor.checks.machine`), in order: `machine.record`,
//! `machine.operation`, `machine.configuration`, `machine.provider`,
//! `machine.state`, `machine.worker.endpoint`, `machine.worker.auth`,
//! `machine.worker.compatibility`, `machine.worker.ping`,
//! `machine.capacity`, `machine.network` (the Sandbox network, read by the
//! worker, or here for `native` on Linux; left out for `native` on macOS).
//! Nothing is started, stopped or written.
//!
//! What a probe finds is read as `machine get --with live` reads it (its
//! state words, reasons and fixes from `layerfs_machine::state`): the
//! provider is asked of the VM alone; a Docker Engine that does not answer
//! is `engine_down`, whose fix starts Docker; a VM the provider does not
//! hold is `missing`; one it reports broken `failed`; a running machine's
//! worker that does not answer is `not_answering`, one of another build
//! `outdated` (each fixed by a stop and a start, the stop forced while
//! Sandboxes are recorded on it: a start does not replace a running
//! machine's worker). A check that does not apply is left out: `native` has
//! no provider or separate data disk, and a stopped machine's guest worker checks
//! are not made (stopped on purpose is not a finding). After a failed
//! check the ones that need it are not made.
//!
//! [`diagnose`] answers `Ok` when its checks found it ready (nothing on it
//! is blocked), else what blocks the things on it (the frame's `blocked_by`
//! reason), or `Err(None)` when a check gave no verdict.
use super::target::{Budget, Checks};
use layerfs_bridge::control::SdkValue;
#[cfg(feature = "machine")]
use layerfs_home::outcome::shell_word;
use std::path::Path;

#[cfg(not(feature = "machine"))]
pub(super) fn diagnose(
    home: &Path,
    name: &str,
    _budget: &Budget,
    checks: &mut Checks,
) -> Result<(), Option<&'static str>> {
    match layerfs_machine::record::MachineRecord::read(home, name) {
        Ok(Some(_)) => checks.pass("machine.record", "its record reads"),
        Ok(None) => {
            checks.fail("machine.record", "it has no record");
            // no fix: the platform to create it with is a choice
            checks.evidence(
                "machine.record",
                SdkValue::map([("record", SdkValue::text("absent"))]),
            );
            return Err(Some("missing"));
        }
        Err(refusal) => {
            checks.fail("machine.record", refusal.message);
            // no fix: see the build with adapters
            checks.evidence(
                "machine.record",
                SdkValue::map([("code", SdkValue::text(&refusal.code))]),
            );
            return Err(Some("failed"));
        }
    }
    checks.fail(
        "machine.configuration",
        "this build has no machine adapters: no request can be made to its provider",
    );
    // no fix: another build is needed
    checks.evidence(
        "machine.configuration",
        SdkValue::map([("adapters", SdkValue::Bool(false))]),
    );
    Err(None)
}

#[cfg(feature = "machine")]
pub(super) fn diagnose(
    home: &Path,
    name: &str,
    budget: &Budget,
    checks: &mut Checks,
) -> Result<(), Option<&'static str>> {
    use layerfs_machine::platform::{HostOs, Route};
    let (record, kind) = record(home, name, checks).map_err(Some)?;
    let sandboxes = crate::inventory::sandboxes_on(home, name).map_or(0, |on| on.len());
    if let Some(gate) = operation(&record, checks) {
        return Err(Some(gate));
    }
    if kind.route(HostOs::current()) == Route::InProcess {
        checks.pass(
            "machine.state",
            "it runs whenever this computer does, as its record says",
        );
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        if name == layerfs_home::names::HOST_MACHINE {
            super::host_worker::diagnose(home, budget, checks);
        }
        // `native` on Linux runs namespaces Sandboxes on this host's
        // network, and only for root: for anyone else it does not apply
        #[cfg(target_os = "linux")]
        if layerfs_sandbox_backend::network::privileged() {
            match (super::network::owned_in(home), super::network::report(home)) {
                (Ok(owned), Ok(report)) => {
                    super::network::machine(&report, (name, sandboxes, true), &owned, checks)
                }
                (Err(why), _) => checks.unknown("machine.network", why),
                (_, Err(refusal)) => checks.unknown("machine.network", refusal.message),
            }
        }
        return Ok(());
    }
    // a Linux Engine's docker worker shares this host's network
    let host_network =
        cfg!(target_os = "linux") && kind == layerfs_machine::platform::Platform::Docker;
    let mut machine = Machine {
        home,
        name,
        budget,
        checks,
        sandboxes,
        host_network,
    };
    machine.probe(&record)
}

/// `machine.record`: its record reads, is this build's, and names a
/// platform this build runs. `native` and `docker` exist without a record.
#[cfg(feature = "machine")]
fn record(
    home: &Path,
    name: &str,
    checks: &mut Checks,
) -> Result<
    (
        layerfs_machine::record::MachineRecord,
        layerfs_machine::platform::Platform,
    ),
    &'static str,
> {
    use layerfs_home::home;
    use layerfs_machine::record::MachineRecord;
    const ID: &str = "machine.record";
    let file = home::paths::machine_record(home, name);
    // a built-in machine with no record yet is the one its first write makes
    let mut written = true;
    let record = match MachineRecord::read(home, name) {
        Ok(Some(record)) => record,
        Ok(None) if layerfs_home::names::BUILT_IN.contains(&name) => match home::opened(home) {
            Ok(file) => {
                written = false;
                MachineRecord::built_in(name, &file.id)
            }
            Err(refusal) => {
                checks.fail(ID, refusal.message);
                // no fix: the home's own file is the problem (`home.layout`)
                checks.evidence(ID, SdkValue::map([("code", SdkValue::text(&refusal.code))]));
                return Err("failed");
            }
        },
        Ok(None) => {
            checks.fail(ID, "it has no record");
            // no fix: `machine create` needs a platform
            checks.evidence(
                ID,
                SdkValue::map([
                    ("record", SdkValue::text("absent")),
                    ("built_in", SdkValue::Bool(false)),
                ]),
            );
            return Err("missing");
        }
        Err(refusal) => {
            checks.fail(ID, refusal.message);
            // no fix: `machine remove` reads the record too and refuses the
            // same error
            checks.evidence(
                ID,
                SdkValue::map([
                    ("code", SdkValue::text(&refusal.code)),
                    ("path", SdkValue::text(file.display().to_string())),
                ]),
            );
            return Err("failed");
        }
    };
    if (written && record.int("v") != Some(home::RECORD_VERSION))
        || record.text("name") != Some(name)
        || !valid_saved_fields(&record)
        || (record.field("op").is_some() && record.op().is_none())
    {
        checks.fail(
            ID,
            "its record is not valid: its version, name, state, operation or sizes",
        );
        // `machine remove` of a record that names a VM deletes the VM and its
        // disk, and refuses a built-in machine: offer it only when it takes
        // records alone
        let records_only = record.nameless() || record.removed().is_some();
        if records_only && !layerfs_home::names::BUILT_IN.contains(&name) {
            checks.fix(
                ID,
                format!("layerfs machine remove {name}", name = shell_word(name)),
            );
        }
        checks.evidence(
            ID,
            SdkValue::map([
                ("problem", SdkValue::text("invalid")),
                ("path", SdkValue::text(file.display().to_string())),
            ]),
        );
        return Err("failed");
    }
    let kind = match record.platform() {
        Ok(kind) => kind,
        Err(refusal) if refusal.code == "Unsupported" => {
            checks.fail(ID, refusal.message);
            checks.fix(
                ID,
                format!("layerfs machine remove {name}", name = shell_word(name)),
            );
            checks.evidence(
                ID,
                SdkValue::map([
                    ("platform", SdkValue::opt_text(record.text("platform"))),
                    ("code", SdkValue::text(&refusal.code)),
                ]),
            );
            return Err("unsupported");
        }
        Err(refusal) => {
            checks.fail(ID, refusal.message);
            // no fix: `machine remove` refuses a platform it does not know
            checks.evidence(ID, SdkValue::map([("code", SdkValue::text(&refusal.code))]));
            return Err("failed");
        }
    };
    checks.pass(ID, format!("its record is valid and names {}", kind.name()));
    checks.evidence(
        ID,
        layerfs_bridge::control::SdkValue::map([
            (
                "platform",
                layerfs_bridge::control::SdkValue::text(kind.name()),
            ),
            (
                "recorded_state",
                layerfs_bridge::control::SdkValue::opt_text(
                    record.recorded_state().map(str::to_owned),
                ),
            ),
        ]),
    );
    Ok((record, kind))
}

/// `machine.operation`: no operation on it is running or left with its
/// outcome not known. A running one is `busy` (the check gives no
/// verdict); an unresolved one is an error whose fix settles it (C13). A
/// certain operation a gone owner left holds nothing, as every read says.
#[cfg(feature = "machine")]
fn operation(
    record: &layerfs_machine::record::MachineRecord,
    checks: &mut Checks,
) -> Option<&'static str> {
    use layerfs_home::op::Standing;
    const ID: &str = "machine.operation";
    let Some(op) = record.op() else {
        checks.pass(ID, "no operation is on it");
        return None;
    };
    let standing = op.standing();
    // a record's own words are shown only when they are this build's
    let what = match (op.name(), PHASES.contains(&op.phase())) {
        (name, true) if NAMES.contains(&name) => format!("{} (phase {})", op.word(), op.phase()),
        (name, false) if NAMES.contains(&name) => op.word().to_owned(),
        _ => "operation".to_owned(),
    };
    let gate = match standing {
        Standing::Running => {
            checks.busy(ID, format!("its {what} is running"));
            Some("busy")
        }
        Standing::Unresolved => {
            checks.fail(ID, format!("the outcome of its {what} is not known"));
            checks.fix(ID, layerfs_machine::state::settle(record.name(), &op));
            Some("unresolved")
        }
        Standing::Stale => {
            checks.pass(
                ID,
                "no operation is running or unresolved; a finished one is still on its record",
            );
            None
        }
    };
    checks.evidence(ID, operation_evidence(&op, standing));
    gate
}

/// The operations a machine's record may hold, and their phases.
#[cfg(feature = "machine")]
const NAMES: [&str; 4] = [
    layerfs_machine::record::CREATE,
    layerfs_machine::record::REMOVE,
    "machine.start",
    "machine.stop",
];
#[cfg(feature = "machine")]
const PHASES: [&str; 4] = ["create", "delete", "start", "stop"];

/// The machine whose provider and worker are probed.
#[cfg(feature = "machine")]
struct Machine<'a> {
    home: &'a Path,
    name: &'a str,
    budget: &'a Budget,
    checks: &'a mut Checks,
    /// How many Sandboxes are recorded on it.
    sandboxes: usize,
    /// Whether its Sandboxes' network is this Linux host's, shared with
    /// other homes and machines.
    host_network: bool,
}

#[cfg(feature = "machine")]
impl Machine<'_> {
    /// Its provider and worker checks, in order. `Err` stops them: with
    /// what blocks the things on it, or `None` when a check gave no
    /// verdict (the records then say).
    fn probe(
        &mut self,
        record: &layerfs_machine::record::MachineRecord,
    ) -> Result<(), Option<&'static str>> {
        use layerfs_machine::platform::State;
        let spec = self.configuration(record)?;
        let status = self.provider(&spec)?.ok_or(None)?;
        self.state(record, status.kernel);
        if status.kernel != Some(State::Running) {
            return Err(Some("stopped"));
        }
        let free = self.worker(status.free_disk)?;
        self.capacity(free);
        Ok(())
    }

    /// `machine.configuration`: its record makes a valid request to its
    /// provider (the home's id included).
    fn configuration(
        &mut self,
        record: &layerfs_machine::record::MachineRecord,
    ) -> Result<layerfs_machine::platform::Spec, Option<&'static str>> {
        const ID: &str = "machine.configuration";
        let made = layerfs_machine::machine_provider_spec(self.home, record);
        match made {
            Ok(spec) => {
                self.checks
                    .pass(ID, "its record makes a valid request to its provider");
                Ok(spec)
            }
            Err(refusal) => {
                let mut fix = refusal.detail.get_text("fix").map(str::to_owned);
                // a record with no 2.0 name: the line that removes what is
                // on it first, as its remove's `InUse` refusal carries
                if record.nameless() {
                    if let Ok(held) = crate::inventory::Held::read(self.home, self.name) {
                        let chain = layerfs_machine::record::no_vm_name_fix;
                        fix = Some(chain(self.name, held.holders()));
                    }
                }
                self.checks.fail(ID, refusal.message);
                // no fix when its refusal names none
                if let Some(fix) = fix {
                    self.checks.fix(ID, fix);
                }
                self.checks.evidence(
                    ID,
                    SdkValue::map([
                        ("code", SdkValue::text(&refusal.code)),
                        ("nameless", SdkValue::Bool(record.nameless())),
                    ]),
                );
                // what is on it is blocked for the reason its state says
                match record.nameless() {
                    true => Err(Some("outdated")),
                    false => Err(Some("failed")),
                }
            }
        }
    }

    /// `machine.provider`: its provider answers, and says whether its VM
    /// or container runs. `None`: the probe gave no verdict.
    fn provider(
        &mut self,
        spec: &layerfs_machine::platform::Spec,
    ) -> Result<Option<layerfs_machine::platform::Status>, Option<&'static str>> {
        use layerfs_bridge::control::SdkValue;
        use layerfs_machine::platform::{self, Code, State};
        use layerfs_machine::state::start_docker;
        use std::time::{Duration, Instant};
        const ID: &str = "machine.provider";
        let name = self.name;
        if self.budget.expired() {
            self.checks
                .timeout(ID, "its time ran out before its provider was asked");
            return Ok(None);
        }
        // the provider is asked of the VM alone: the worker's hello is ours
        let spec = platform::Spec {
            worker: None,
            ..spec.clone()
        };
        let stop = Instant::now() + self.budget.limit(Duration::from_secs(10));
        let status = match platform::status_within(&spec, stop) {
            Ok(status) => status,
            Err(error) if error.code == Code::Unsupported => {
                self.checks.fail(ID, error.message);
                // no fix: the platform runs on another host
                self.checks.evidence(ID, provider_failed(error.code, &spec));
                return Err(Some("unsupported"));
            }
            Err(error) if self.budget.expired() || error.code == Code::Unknown => {
                match self.budget.expired() {
                    true => self.checks.timeout(ID, error.message),
                    false => self.checks.lost(ID, error.message),
                }
                return Ok(None);
            }
            Err(error) => {
                self.checks.fail(ID, error.message);
                // no fix: no single cause; the provider's words are the message
                self.checks.evidence(ID, provider_failed(error.code, &spec));
                return Err(Some("failed"));
            }
        };
        let evidence = SdkValue::map([
            (
                "state",
                status
                    .kernel
                    .map_or(SdkValue::Null, |state| SdkValue::text(state.as_str())),
            ),
            ("runtime", SdkValue::text(&spec.vm_name)),
            (
                "tool_version",
                SdkValue::opt_text(status.tool_version.clone()),
            ),
        ]);
        let gate = match status.kernel {
            None => {
                let why = (status.down_reason.clone())
                    .unwrap_or_else(|| "its container engine does not answer".to_owned());
                self.checks.fail(ID, why);
                self.checks.fix(ID, start_docker());
                Some("engine_down")
            }
            Some(State::Missing) => {
                let message = format!("its provider holds no VM named {}", spec.vm_name);
                self.checks.fail(ID, message);
                // the line that removes what is on it first, as its
                // remove's `InUse` refusal carries (area 10)
                let held = crate::inventory::Held::read(self.home, name);
                let fix = match held {
                    Ok(held) => layerfs_machine::record::no_vm_name_fix(name, held.holders()),
                    Err(_) => format!("layerfs machine remove {name}", name = shell_word(name)),
                };
                self.checks.fix(ID, fix);
                Some("missing")
            }
            Some(State::Failed) => {
                self.checks.fail(ID, "its provider reports it broken");
                self.checks.fix(
                    ID,
                    format!("layerfs machine start {name}", name = shell_word(name)),
                );
                Some("failed")
            }
            Some(State::Stopped) => {
                self.checks.pass(ID, "its provider answers; it is stopped");
                None
            }
            Some(State::Running) => {
                self.checks.pass(ID, "its provider answers; it runs");
                None
            }
        };
        self.checks.evidence(ID, evidence);
        match gate {
            Some(gate) => Err(Some(gate)),
            None => Ok(Some(status)),
        }
    }

    /// `machine.state`: the state its record says agrees with what the
    /// provider reports. A record a live read would change is a warning
    /// whose fix is that read.
    fn state(
        &mut self,
        record: &layerfs_machine::record::MachineRecord,
        kernel: Option<layerfs_machine::platform::State>,
    ) {
        use layerfs_home::state::{Reason, State};
        const ID: &str = "machine.state";
        let name = self.name;
        let recorded = record.recorded(self.sandboxes);
        let seen = kernel.map_or("unknown", |state| state.as_str());
        // what the provider says of the VM while the record holds `recorded`
        let expected = match &recorded {
            State::Ready => Some("running"),
            State::Stopped => Some("stopped"),
            State::Broken { reason, .. } => match reason {
                Reason::NotAnswering | Reason::Outdated => Some("running"),
                Reason::Missing => Some("missing"),
                Reason::Failed => Some("failed"),
                _ => None,
            },
            _ => None,
        };
        let said = match &recorded {
            State::Broken { reason, .. } => format!("broken ({})", reason.word()),
            other => other.word().to_owned(),
        };
        if expected == Some(seen) {
            self.checks.pass(
                ID,
                format!("its record says {said}, and its provider reports it {seen}"),
            );
        } else {
            self.checks.warning(
                ID,
                format!("its record says {said}, but its provider reports it {seen}"),
            );
            self.checks.fix(
                ID,
                format!(
                    "layerfs machine get {name} --with live",
                    name = shell_word(name)
                ),
            );
            self.checks.evidence(
                ID,
                SdkValue::map([
                    ("recorded", SdkValue::text(said)),
                    ("observed", SdkValue::text(seen)),
                ]),
            );
        }
    }

    /// The worker of a running machine: its saved endpoint, its hello, its
    /// build and one ping. Answers the data disk's free bytes, from the
    /// provider (`free`) or the worker's ping.
    fn worker(&mut self, free: Option<u64>) -> Result<Option<u64>, Option<&'static str>> {
        use layerfs_machine::state::replace_worker;
        use layerfs_machine::worker::{diagnostic_connect, Endpoint};
        use std::time::{Duration, Instant};
        let name = self.name;
        let start = format!("layerfs machine start {name}", name = shell_word(name));
        let replace = replace_worker(name, self.sandboxes);
        let endpoint = match layerfs_home::secrets::worker_secret(self.home, name) {
            Ok(Some(value))
                if !value.secret.is_empty()
                    && value.address.ip().is_loopback()
                    && value.address.port() != 0 =>
            {
                Endpoint {
                    address: value.address,
                    secret: value.secret,
                }
            }
            Ok(_) => {
                self.failed(
                    "machine.worker.endpoint",
                    "it runs, and no worker endpoint and secret are saved for it",
                    &start,
                    saved("absent"),
                );
                return Err(Some("not_answering"));
            }
            Err(_) => {
                self.failed(
                    "machine.worker.endpoint",
                    "its saved worker endpoint does not read",
                    &start,
                    saved("unreadable"),
                );
                return Err(Some("not_answering"));
            }
        };
        self.checks.pass(
            "machine.worker.endpoint",
            "a worker endpoint and secret are saved for it",
        );
        let stop = Instant::now() + self.budget.limit(Duration::from_secs(5));
        let worker = match diagnostic_connect(&endpoint, stop) {
            Ok(worker) => worker,
            Err(error) => {
                if self.budget.expired() {
                    self.checks.timeout(
                        "machine.worker.auth",
                        "its time ran out before the worker answered",
                    );
                    return Err(None);
                }
                if matches!(error.code.as_str(), "Timeout" | "Unavailable") {
                    // a start is refused over a running machine's silent worker
                    let message = format!("its worker does not answer: {}", error.message);
                    let evidence = code_of(&error.code);
                    self.failed("machine.worker.auth", &message, &replace, evidence);
                } else {
                    let message = format!("its worker refused the saved secret ({})", error.code);
                    let evidence = code_of(&error.code);
                    self.failed("machine.worker.auth", &message, &replace, evidence);
                }
                return Err(Some("not_answering"));
            }
        };
        self.checks.pass(
            "machine.worker.auth",
            "its worker accepts the secret and answers hello",
        );
        self.checks
            .evidence("machine.worker.auth", worker.hello.to_sdk());
        if worker.hello.expected().is_some() {
            let message = match self.sandboxes {
                0 => "worker is from another build".to_owned(),
                1 => "worker is from another build; 1 Sandbox is on it".to_owned(),
                n => format!("worker is from another build; {n} Sandboxes are on it"),
            };
            let evidence = worker.hello.to_sdk();
            self.failed("machine.worker.compatibility", &message, &replace, evidence);
            return Err(Some("outdated"));
        }
        self.checks.pass(
            "machine.worker.compatibility",
            "its worker is from this build",
        );
        let free = self.ping(worker, free, &start, &replace)?;
        self.network(&endpoint, stop);
        Ok(free)
    }

    /// `machine.network`: its worker reads its Sandbox network
    /// (`network.inspect`), on a connection of its own. A worker that does
    /// not answer leaves the check without a verdict.
    fn network(&mut self, endpoint: &layerfs_machine::worker::Endpoint, stop: std::time::Instant) {
        use layerfs_machine::worker::{diagnostic_connect, DiagnosticOperation};
        const ID: &str = "machine.network";
        let answer = diagnostic_connect(endpoint, stop).and_then(|worker| {
            worker.diagnose(
                DiagnosticOperation::Network,
                layerfs_bridge::control::SdkValue::Null,
                &mut |_| Ok(()),
            )
        });
        // every Sandbox this home records, on any machine: on a Linux host
        // `native`'s links are in the namespace a Linux Engine's worker
        // reads; with a record that cannot be read, no link is a leftover
        match (answer, super::network::owned_in(self.home)) {
            (Ok(report), Ok(owned)) => super::network::machine(
                &report,
                (self.name, self.sandboxes, self.host_network),
                &owned,
                self.checks,
            ),
            (Ok(_), Err(why)) => self.checks.unknown(ID, why),
            (Err(error), _) => self.checks.unknown(
                ID,
                format!(
                    "its worker did not read its Sandbox network: {}",
                    error.message
                ),
            ),
        }
    }

    /// `machine.worker.ping`: the worker answers one read-only request with
    /// its process and this build's version.
    fn ping(
        &mut self,
        worker: layerfs_machine::worker::DiagnosticWorker,
        free: Option<u64>,
        start: &str,
        replace: &str,
    ) -> Result<Option<u64>, Option<&'static str>> {
        const ID: &str = "machine.worker.ping";
        let answer = worker.ping();
        match answer {
            Ok(value)
                if value.get_text("kind") == Some("worker")
                    && value.get_int("pid").is_some_and(|pid| pid > 0)
                    && value.get_text("version") == Some(env!("CARGO_PKG_VERSION")) =>
            {
                self.checks.pass(ID, "its worker answers a request");
                let pinged = value
                    .get_int("free_disk")
                    .and_then(|b| u64::try_from(b).ok());
                self.checks.evidence(ID, value);
                Ok(free.or(pinged))
            }
            Ok(value) => {
                let said = |key| {
                    let text = value.get_text(key);
                    SdkValue::opt_text(text.map(|t| t.chars().take(64).collect::<String>()))
                };
                let evidence =
                    SdkValue::map([("kind", said("kind")), ("version", said("version"))]);
                self.failed(
                    ID,
                    "its worker's answer names another process or build",
                    replace,
                    evidence,
                );
                Err(Some("outdated"))
            }
            Err(error)
                if self.budget.expired()
                    || matches!(error.code.as_str(), "Timeout" | "Unavailable") =>
            {
                self.checks.lost(ID, "its worker's answer did not arrive");
                Err(None)
            }
            Err(error) => {
                let message = format!("its worker refused a request ({})", error.code);
                self.failed(ID, &message, start, code_of(&error.code));
                Err(Some("not_answering"))
            }
        }
    }

    /// `machine.capacity`: its data disk has room. Its evidence adds the
    /// bytes of its image cache `machine prune --unused` frees
    /// (`reclaimable_image_bytes`, from the machine's image listing); that
    /// prune is the fix only when there are such bytes.
    fn capacity(&mut self, free: Option<u64>) {
        use layerfs_bridge::control::SdkValue;
        const ID: &str = "machine.capacity";
        let reclaimable = layerfs_home::names::MachineName::parse(self.name)
            .ok()
            .and_then(|machine| layerfs_sandbox::images::list(self.home, &machine, false).ok())
            .map(|listed| layerfs_sandbox::images::reclaimable(&listed));
        let fix = reclaimable
            .filter(|bytes| *bytes > 0)
            .map(|_| format!("layerfs machine prune {} --unused", shell_word(self.name)));
        let bytes = reclaimable.and_then(|n| i64::try_from(n).ok());
        let evidence = vec![(
            "reclaimable_image_bytes",
            bytes.map_or(SdkValue::Null, SdkValue::Int),
        )];
        match free {
            Some(free) => {
                super::capacity::room(self.checks, ID, "its data disk", free, fix, evidence)
            }
            None => self.checks.unknown(
                ID,
                "neither its provider nor its worker measured its data disk",
            ),
        }
    }

    /// An error on check `id` with `fix` and `evidence`.
    fn failed(&mut self, id: &str, message: &str, fix: &str, evidence: SdkValue) {
        self.checks.fail(id, message);
        self.checks.fix(id, fix);
        self.checks.evidence(id, evidence);
    }
}

/// The evidence of a worker endpoint that cannot be used: it is `absent` or
/// `unreadable`.
#[cfg(feature = "machine")]
fn saved(word: &str) -> SdkValue {
    SdkValue::map([("saved", SdkValue::text(word))])
}

/// The evidence of a worker's answer that names `code`, a word of the
/// worker protocol.
#[cfg(feature = "machine")]
fn code_of(code: &str) -> SdkValue {
    let code: String = code.chars().take(64).collect();
    SdkValue::map([("code", SdkValue::text(code))])
}

/// The evidence of a provider that failed: its error code and the VM or
/// container asked.
#[cfg(feature = "machine")]
fn provider_failed(
    code: layerfs_machine::platform::Code,
    spec: &layerfs_machine::platform::Spec,
) -> SdkValue {
    SdkValue::map([
        ("code", SdkValue::text(code.as_str())),
        ("runtime", SdkValue::text(&spec.vm_name)),
    ])
}

#[cfg(feature = "machine")]
fn valid_saved_fields(record: &layerfs_machine::record::MachineRecord) -> bool {
    if record.field("state").is_some()
        && !matches!(
            record.recorded_state(),
            Some("running" | "stopped" | "failed")
        )
    {
        return false;
    }
    if record.field("reason").is_some() && record.reason().is_none() {
        return false;
    }
    for field in ["cpus", "memory", "disk"] {
        if record.field(field).is_some()
            && !record
                .int(field)
                .is_some_and(|value| value > 0 && (field != "cpus" || value <= i64::from(u32::MAX)))
        {
            return false;
        }
    }
    if let Some(lasting) = record.field("lasting") {
        if lasting.get("port").is_some()
            && !lasting
                .get_int("port")
                .is_some_and(|port| (1..=65535).contains(&port))
        {
            return false;
        }
    }
    true
}

#[cfg(feature = "machine")]
fn operation_evidence(
    op: &layerfs_home::op::Op,
    standing: layerfs_home::op::Standing,
) -> layerfs_bridge::control::SdkValue {
    use layerfs_bridge::control::SdkValue;
    use layerfs_home::op::Standing;
    let id = op.id();
    let valid_id = id.len() == 19
        && id
            .strip_prefix("op-")
            .is_some_and(|suffix| suffix.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let known = |said: &str, words: &[&str]| match words.contains(&said) {
        true => SdkValue::text(said),
        false => SdkValue::Null,
    };
    SdkValue::map([
        (
            "standing",
            SdkValue::text(match standing {
                Standing::Running => "running",
                Standing::Unresolved => "unresolved",
                Standing::Stale => "stale",
            }),
        ),
        ("uncertain", SdkValue::Bool(op.uncertain())),
        (
            "id",
            match valid_id {
                true => SdkValue::text(id),
                false => SdkValue::Null,
            },
        ),
        ("name", known(op.name(), &NAMES)),
        ("phase", known(op.phase(), &PHASES)),
    ])
}

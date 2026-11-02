//! The one support table (#535 §7.3, G1, G7): for each platform, host and
//! Store kind, whether a Sandbox is supported or the fixed reason it is
//! refused. It encodes what is allowed; phase 5a enabled PostgreSQL
//! Sandboxes on native (macOS and Linux), docker and ssh, and phases 5b and
//! 5c flip the VM platforms' rows here once their receipts exist (G7);
//! `connect/SUPPORT.json` (rendered by [`support_json`], compared by a
//! test) and `machine.platforms` follow.
use crate::options::OptSpec;
use crate::platform::{ExecutorKind, HostOs, How, KernelRoute, Platform};
pub use layerfs_bridge::StoreKind;
use layerfs_sandbox::json::quoted;
use layerfs_sandbox::Backend;

/// Where a Store's calls run for a machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallSite {
    /// The caller's own process.
    InProcess,
    /// The machine's worker.
    Worker,
}

/// PostgreSQL on one platform and host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PostgresqlSupport {
    /// Where its Store calls run: always the caller's process (#533 G2).
    pub store_calls: CallSite,
    /// Whether a Sandbox of a PostgreSQL Store runs here.
    pub sandbox: bool,
    /// Why not, when it does not.
    pub reason: Option<&'static str>,
}

/// One platform on one host: the row `machine.platforms` returns, minus
/// its live fields (`ready`, `missing`, `version`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SupportRow {
    pub platform: Platform,
    pub host: HostOs,
    pub how: How,
    /// The Sandbox backend; `None` when there is none or the platform is
    /// not on this host.
    pub backend: Option<Backend>,
    pub kernel: Option<KernelRoute>,
    /// Where a sqlite Store's calls run; `None` when the platform is not on
    /// this host.
    pub sqlite: Option<CallSite>,
    pub postgresql: PostgresqlSupport,
    pub options: &'static [OptSpec],
    /// The platform and how its Sandboxes run.
    pub description: String,
}

/// A machine's `supports` (`machine.list`, `machine.get`): its platform's
/// row applied to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Supports {
    pub sqlite: bool,
    pub postgresql_sandbox: bool,
    pub reason: Option<&'static str>,
}

const OTHER_HOST: &str = "LayerFS names no host other than macOS, Linux and Windows";
const NO_BACKEND: &str = "no Sandbox backend on Windows (O5)";
/// G7: no live receipt records these platforms' host aliases yet; `reach`
/// refuses a loopback URL with the same text.
pub(crate) const APPLE_CONTAINER_ALIAS: &str = "apple-container host alias not verified (G7)";
pub(crate) const MICROSANDBOX_ALIAS: &str = "microsandbox host alias not verified (G7)";

/// Why `platform` cannot be on `host`, or `None` where it can.
fn host_refusal(platform: Platform, host: HostOs) -> Option<&'static str> {
    if host == HostOs::Other {
        return Some(OTHER_HOST);
    }
    if platform.profile().hosts.contains(&host) {
        return None;
    }
    Some(match platform {
        Platform::Docker => "docker through WSL2 is not designed yet",
        Platform::AppleContainer => "apple-container runs on macOS hosts only",
        Platform::Lima => "lima runs on macOS hosts only",
        Platform::Firecracker => "firecracker runs on macOS hosts only, in a nested lima machine",
        Platform::Native | Platform::Microsandbox | Platform::Ssh => OTHER_HOST,
    })
}

/// Why a PostgreSQL Sandbox does not run on `platform` (on a host it is
/// on, with a backend), or `None` where it does. #535 phase 5a: native
/// (Linux namespaces with no bind), docker and ssh (whose `reach` refuses
/// a loopback URL) run them; the VM platforms wait for G7 receipts.
fn postgresql_refusal(platform: Platform, host: HostOs) -> Option<&'static str> {
    match (platform, host) {
        (Platform::Native, HostOs::Macos | HostOs::Linux)
        | (Platform::Docker | Platform::Ssh, _) => None,
        (Platform::Native, _) => Some(NO_BACKEND),
        (Platform::Lima, _) => Some("lima host alias not verified (G7)"),
        (Platform::AppleContainer, _) => Some(APPLE_CONTAINER_ALIAS),
        (Platform::Microsandbox, _) => Some(MICROSANDBOX_ALIAS),
        (Platform::Firecracker, _) => Some("outbound NAT and host alias not verified (G7)"),
    }
}

/// Whether a Sandbox of a `store` Store may run on a `platform` machine on
/// `host`; the refusal is the row's `reason`, word for word.
pub fn sandbox_allowed(
    platform: Platform,
    host: HostOs,
    store: StoreKind,
) -> Result<(), &'static str> {
    if let Some(reason) = host_refusal(platform, host) {
        return Err(reason);
    }
    if platform.backend(host).is_none() {
        return Err(NO_BACKEND);
    }
    match store {
        StoreKind::Sqlite => Ok(()),
        StoreKind::Postgresql => postgresql_refusal(platform, host).map_or(Ok(()), Err),
    }
}

/// `platform`'s row on `host`.
pub fn support(platform: Platform, host: HostOs) -> SupportRow {
    let profile = platform.profile();
    let on_host = host_refusal(platform, host).is_none();
    let backend = platform.backend(host).filter(|_| on_host);
    let postgresql = sandbox_allowed(platform, host, StoreKind::Postgresql);
    SupportRow {
        platform,
        host,
        how: profile.how,
        backend,
        kernel: platform.kernel(host).filter(|_| on_host),
        sqlite: on_host.then(|| match platform.executor(host) {
            ExecutorKind::Local => CallSite::InProcess,
            ExecutorKind::Worker => CallSite::Worker,
        }),
        postgresql: PostgresqlSupport {
            store_calls: CallSite::InProcess,
            sandbox: postgresql.is_ok(),
            reason: postgresql.err(),
        },
        options: profile.options,
        description: match backend {
            Some(backend) => format!("{}; {}", profile.description, backend.description()),
            None => profile.description.to_owned(),
        },
    }
}

/// Every platform's row on `host`, in [`Platform::ALL`] order.
pub fn platforms(host: HostOs) -> Vec<SupportRow> {
    Platform::ALL
        .into_iter()
        .map(|p| support(p, host))
        .collect()
}

impl SupportRow {
    /// The row applied to one machine of its platform.
    pub fn supports(&self) -> Supports {
        let sqlite = sandbox_allowed(self.platform, self.host, StoreKind::Sqlite);
        Supports {
            sqlite: sqlite.is_ok(),
            postgresql_sandbox: self.postgresql.sandbox,
            reason: self.postgresql.reason,
        }
    }
}

impl CallSite {
    /// Its stable name: `in_process` or `worker`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InProcess => "in_process",
            Self::Worker => "worker",
        }
    }
}

/// `connect/SUPPORT.json`: every named host's rows, keys in a fixed order.
pub fn support_json() -> String {
    let mut out = String::from("{\n  \"generated_from\": \"connect/crates/layerfs-machine/src/support.rs\",\n  \"hosts\": [\n");
    for (h, host) in HostOs::NAMED.into_iter().enumerate() {
        out.push_str("    {\n      \"host\": ");
        text(&mut out, host.as_str());
        out.push_str(",\n      \"platforms\": [\n");
        let rows = platforms(host);
        for (r, row) in rows.iter().enumerate() {
            render(&mut out, row);
            out.push_str(if r + 1 < rows.len() { ",\n" } else { "\n" });
        }
        out.push_str("      ]\n    }");
        out.push_str(if h + 1 < HostOs::NAMED.len() {
            ",\n"
        } else {
            "\n"
        });
    }
    out.push_str("  ]\n}\n");
    out
}

fn render(out: &mut String, row: &SupportRow) {
    let pad = "          ";
    out.push_str("        {\n");
    field(out, pad, "platform", Some(row.platform.name()));
    field(out, pad, "how", Some(row.how.as_str()));
    field(out, pad, "backend", row.backend.map(Backend::stored_name));
    field(out, pad, "kernel", row.kernel.map(KernelRoute::as_str));
    field(out, pad, "sqlite", row.sqlite.map(CallSite::as_str));
    out.push_str(pad);
    out.push_str("\"postgresql\": { \"store_calls\": ");
    text(out, row.postgresql.store_calls.as_str());
    out.push_str(", \"sandbox\": ");
    out.push_str(if row.postgresql.sandbox {
        "true"
    } else {
        "false"
    });
    out.push_str(", \"reason\": ");
    maybe(out, row.postgresql.reason);
    out.push_str(" },\n");
    out.push_str(pad);
    out.push_str("\"options\": [");
    for (i, option) in row.options.iter().enumerate() {
        out.push_str(if i == 0 { " " } else { ", " });
        out.push_str("{ \"name\": ");
        text(out, option.name);
        out.push_str(", \"kind\": ");
        text(out, option.kind.as_str());
        out.push_str(" }");
    }
    out.push_str(if row.options.is_empty() {
        "],\n"
    } else {
        " ],\n"
    });
    out.push_str(pad);
    out.push_str("\"description\": ");
    text(out, &row.description);
    out.push_str("\n        }");
}

fn field(out: &mut String, pad: &str, name: &str, value: Option<&str>) {
    out.push_str(pad);
    text(out, name);
    out.push_str(": ");
    maybe(out, value);
    out.push_str(",\n");
}

fn maybe(out: &mut String, value: Option<&str>) {
    match value {
        Some(value) => text(out, value),
        None => out.push_str("null"),
    }
}

fn text(out: &mut String, value: &str) {
    let mut bytes = Vec::new();
    // writing to a Vec cannot fail
    let _ = quoted(&mut bytes, value);
    out.push_str(&String::from_utf8_lossy(&bytes));
}

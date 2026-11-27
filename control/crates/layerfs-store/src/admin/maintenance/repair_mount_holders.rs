//! `store.repair_mount_holders` where the Store's calls run: in this process
//! for a PostgreSQL Store or a SQLite Store on this computer, or in the
//! machine's worker (`store.repair`) for a SQLite Store there. The caller's
//! side resolves the project and the cursor ([`Repair`] is what crosses);
//! this side reads the Branch's mount intents and the Init, merge and
//! removal intents at the Branch and its project, judges each with the same
//! owner check and place view admission uses, and acts on one explicit op.
//! An operation row records no place, so only its owner can prove it gone:
//! one left by an owner in another PID namespace or on another host would
//! otherwise hold its scopes until the owner's boot ends (#776 G1).
//!
//! `list` and `preview` write nothing: one page of holders by op, each with
//! its row `status` (`live`, `uncertain`) and the `liveness` admission would
//! give it now (`live`, `uncertain`, `dead`). `settle` marks one op uncertain
//! and refuses a holder that still looks `live` unless forced; `release`
//! retains a settled row as the durable `done` audit; `settle_legacy` marks
//! only mount rows without owner or place evidence; `audit` reads one page of
//! the Branch's retained rows. Nothing is automatic.
use crate::admin::open::args::OpenArgs;
use crate::admin::open::service::with_service;
use crate::data::branch::branch_named;
use layerfs_history::{
    GcAuditAfter, GcIntent, GcIntentKind, GcScopeKind, Liveness, MountHolderView,
};
use layerfs_home::reply::outcome::{failed, refused};
use layerfs_protocol::sdk::{hex, unhex};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use layerfs_store_service::history::gc_admission;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The actions the call takes.
pub const ACTIONS: [&str; 6] = [
    "list",
    "preview",
    "settle",
    "release",
    "settle_legacy",
    "audit",
];

/// One repair as the side that holds the Store runs it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Repair {
    /// The project's name in its Store.
    pub project: String,
    /// The Branch whose holders it repairs.
    pub branch: String,
    /// One of [`ACTIONS`].
    pub action: String,
    /// The holder `settle` and `release` act on.
    pub op: Option<[u8; 16]>,
    /// `settle` only a row without owner or place evidence.
    pub legacy: bool,
    /// `settle` a holder that still looks `live`.
    pub force: bool,
    /// The operator label a release records (empty: `local`).
    pub operator: String,
    /// Where the page starts, from the caller's cursor: the last holder op
    /// listed (`list`, `preview`) or the last audit row's
    /// `<updated_ms>:<op>` (`audit`).
    pub after: Option<SdkValue>,
    /// The rows of one page.
    pub limit: u32,
}

impl Repair {
    /// The request as a worker frame's fields.
    #[must_use]
    pub fn to_fields(&self) -> Vec<(String, SdkValue)> {
        let text = |value: &str| SdkValue::text(value);
        vec![
            ("project".to_owned(), text(&self.project)),
            ("branch".to_owned(), text(&self.branch)),
            ("action".to_owned(), text(&self.action)),
            (
                "op".to_owned(),
                self.op.map_or(SdkValue::Null, |op| text(&hex(&op))),
            ),
            ("legacy".to_owned(), SdkValue::Bool(self.legacy)),
            ("force".to_owned(), SdkValue::Bool(self.force)),
            ("operator".to_owned(), text(&self.operator)),
            ("after".to_owned(), self.after.clone().unwrap_or_default()),
            ("limit".to_owned(), SdkValue::Int(i64::from(self.limit))),
        ]
    }

    /// The request a worker frame's `args` carry; a missing name or a
    /// malformed op is `InvalidInput`.
    pub fn read(args: &SdkValue) -> Result<Self> {
        let named = |name: &str| {
            args.get_text(name)
                .map(str::to_owned)
                .ok_or_else(|| refused("InvalidInput", format!("a Store repair names its {name}")))
        };
        Ok(Self {
            project: named("project")?,
            branch: named("branch")?,
            action: named("action")?,
            op: op_of(args.get_text("op"))?,
            legacy: args.get_bool("legacy"),
            force: args.get_bool("force"),
            operator: args.get_text("operator").unwrap_or("").to_owned(),
            after: args.get("after").filter(|v| **v != SdkValue::Null).cloned(),
            limit: (args.get_int("limit"))
                .and_then(|limit| u32::try_from(limit).ok())
                .unwrap_or(100),
        })
    }
}

/// A holder op as given: 16 bytes of hex, or none.
pub fn op_of(text: Option<&str>) -> Result<Option<[u8; 16]>> {
    match text {
        None | Some("") => Ok(None),
        Some(text) => unhex(text)
            .and_then(|bytes| bytes.try_into().ok())
            .map(Some)
            .ok_or_else(|| refused("InvalidInput", "op must be 16 bytes of hex")),
    }
}

/// Runs `request` on the Store `open` names, with the place view of its
/// caller's home (or the one a worker frame installed): `{ action, items,
/// next }` for a page (`next`, the place the next page starts from, or null),
/// `{ action, operation, status }` for an op, `{ action, settled }` for
/// `settle_legacy`.
pub fn repair_at(open: &OpenArgs, request: &Repair) -> Result<SdkValue> {
    if !ACTIONS.contains(&request.action.as_str()) {
        return Err(refused(
            "InvalidInput",
            format!("action must be one of {}", ACTIONS.join(", ")),
        ));
    }
    if request.action == "audit" {
        return audit(open, request);
    }
    let branch = branch_named(open, &request.project, &request.branch)?;
    crate::admin::route::place_view::scoped(open, || {
        with_service(open, |service| holders(service, branch, request))
    })
}

/// One keyset page of the Branch's retained `done` rows, oldest first by
/// `(updated_ms, op)`; the last row's place travels in the cursor, so a
/// release between two pages repeats no row and skips none of the Branch's
/// own; only a row of a Branch created again under the same names can sort
/// below rows already paged (see `GcAuditAfter`).
fn audit(open: &OpenArgs, request: &Repair) -> Result<SdkValue> {
    let after = audit_after(request.after.as_ref())?;
    with_service(open, |service| {
        let read = gc_admission::done_intents_branch(
            service.store.history(),
            &request.project,
            &request.branch,
            after,
            request.limit,
        )
        .map_err(failed)?;
        let items = read.items.iter().map(gc_admission::done_value).collect();
        let next = read.next.map_or(SdkValue::Null, |next| {
            SdkValue::text(format!("{}:{}", next.updated_ms, hex(&next.op_id)))
        });
        Ok(SdkValue::map([
            ("action", SdkValue::text("audit")),
            ("items", SdkValue::List(items)),
            ("next", next),
        ]))
    })
}

/// The audit page's start as the cursor carries it, `<updated_ms>:<op>`;
/// anything else is `InvalidInput`, as a malformed holder op is.
fn audit_after(after: Option<&SdkValue>) -> Result<Option<GcAuditAfter>> {
    let Some(after) = after else {
        return Ok(None);
    };
    let malformed = || refused("InvalidInput", "the audit cursor is malformed");
    let SdkValue::Text(text) = after else {
        return Err(malformed());
    };
    let (updated_ms, op) = text.split_once(':').ok_or_else(malformed)?;
    let updated_ms = updated_ms.parse::<i64>().map_err(|_| malformed())?;
    let op_id = unhex(op)
        .and_then(|bytes| <[u8; 16]>::try_from(bytes).ok())
        .ok_or_else(malformed)?;
    Ok(Some(GcAuditAfter { updated_ms, op_id }))
}

/// The actions on the live intents that hold the Branch: its mounts, and the
/// Init, merge and removal operations at the Branch or its project.
fn holders(
    service: &layerfs_store_service::StoreService,
    branch: layerfs_history::BranchId,
    request: &Repair,
) -> Result<SdkValue> {
    let catalog = service.store.history();
    let operation = |intent: &GcIntent| {
        matches!(
            intent.kind,
            GcIntentKind::Init | GcIntentKind::Merge | GcIntentKind::Removal
        )
    };
    let mut mounts: Vec<GcIntent> = (catalog.gc_intents(GcScopeKind::Branch, branch.as_slice()))
        .map_err(failed)?
        .into_iter()
        .filter(|intent| intent.kind == GcIntentKind::Mount || operation(intent))
        .collect();
    if let Some(record) = catalog.branch(branch).map_err(failed)? {
        let project = catalog.gc_intents(GcScopeKind::Project, record.stack.as_slice());
        mounts.extend(project.map_err(failed)?.into_iter().filter(operation));
    }
    mounts.sort_by_key(|intent| intent.op_id);
    mounts.dedup_by_key(|intent| intent.op_id);
    let action = request.action.as_str();
    if matches!(action, "list" | "preview") {
        return page(catalog, &mounts, request);
    }
    if action == "settle_legacy" {
        let mut settled = 0i64;
        for intent in mounts.iter().filter(|i| legacy(i)) {
            // a row released since the listing is not counted
            if catalog.gc_mark_uncertain(&intent.op_id).map_err(failed)? {
                settled += 1;
            }
        }
        return Ok(SdkValue::map([
            ("action", SdkValue::text(action)),
            ("settled", SdkValue::Int(settled)),
        ]));
    }
    let op = request
        .op
        .ok_or_else(|| refused("InvalidInput", format!("{action} needs the holder op")))?;
    let shown = hex(&op);
    let found = (mounts.iter().find(|intent| intent.op_id == op)).ok_or_else(|| {
        refused(
            "NotFound",
            format!("no holder {shown} at this Branch or its project"),
        )
    })?;
    let noun = found.kind.as_str();
    let state = match action {
        "settle" => {
            settle(catalog, found, request)?;
            "uncertain"
        }
        _ => {
            if !found.uncertain {
                return Err(refused(
                    "Conflict",
                    format!("{noun} holder {shown} is not settled: settle it before release"),
                ));
            }
            let operator = operator_label(&request.operator)?;
            let evidence = format!("action=release;operator={operator};op={shown}");
            if !catalog.gc_release_done(&op, &evidence).map_err(failed)? {
                return Err(refused(
                    "NotFound",
                    format!("{noun} holder {shown} was already released"),
                ));
            }
            "done"
        }
    };
    Ok(SdkValue::map([
        ("action", SdkValue::text(action)),
        ("operation", SdkValue::text(shown)),
        ("status", SdkValue::text(state)),
    ]))
}

/// Marks `found` uncertain. A holder the owner check and place view still
/// find `live` is refused `InUse` unless forced; `--legacy` settles only a
/// mount row without owner or place evidence, which nothing can judge. A row its
/// owner (or another operator) released since the listing is `NotFound`,
/// as a second release is.
fn settle(
    catalog: &dyn layerfs_history::HistoryCatalog,
    found: &GcIntent,
    request: &Repair,
) -> Result<()> {
    let noun = found.kind.as_str();
    if request.legacy && !legacy(found) {
        return Err(refused(
            "InvalidInput",
            "this holder is not a legacy mount row; --legacy settles only those",
        ));
    }
    if !(request.force || request.legacy) {
        let judged =
            gc_admission::liveness(catalog, std::slice::from_ref(found)).map_err(failed)?;
        if judged.first() == Some(&Liveness::Live) {
            return Err(refused(
                "InUse",
                format!(
                    "{noun} holder {} still looks live: nothing proves its owner gone; \
                     settle it with --force once you know it is",
                    hex(&found.op_id)
                ),
            ));
        }
    }
    if !catalog.gc_mark_uncertain(&found.op_id).map_err(failed)? {
        return Err(refused(
            "NotFound",
            format!("{noun} holder {} was already released", hex(&found.op_id)),
        ));
    }
    Ok(())
}

/// One page of holders after the cursor's op, each judged now.
fn page(
    catalog: &dyn layerfs_history::HistoryCatalog,
    mounts: &[GcIntent],
    request: &Repair,
) -> Result<SdkValue> {
    let after = match &request.after {
        Some(SdkValue::Text(text)) => op_of(Some(text))?,
        _ => None,
    };
    let mut left = mounts
        .iter()
        .filter(|intent| after.is_none_or(|after| intent.op_id > after));
    let shown: Vec<GcIntent> = left
        .by_ref()
        .take(request.limit as usize)
        .cloned()
        .collect();
    let next = match (left.next(), shown.last()) {
        (Some(_), Some(last)) => SdkValue::text(hex(&last.op_id)),
        _ => SdkValue::Null,
    };
    let judged = gc_admission::liveness(catalog, &shown).map_err(failed)?;
    let items = (shown.iter().zip(judged))
        .map(|(intent, liveness)| holder_row(intent, liveness))
        .collect();
    Ok(SdkValue::map([
        ("action", SdkValue::text(&request.action)),
        ("items", SdkValue::List(items)),
        ("next", next),
    ]))
}

/// The word a [`Liveness`] is shown as.
#[must_use]
pub fn liveness_word(liveness: Liveness) -> &'static str {
    match liveness {
        Liveness::Live => "live",
        Liveness::Uncertain => "uncertain",
        Liveness::Dead => "dead",
    }
}

/// True for the rows `settle_legacy` may mark: a mount row without owner or
/// place evidence. An operation row records no place, so it is never one.
fn legacy(intent: &GcIntent) -> bool {
    intent.kind == GcIntentKind::Mount && MountHolderView::of(intent).legacy()
}

/// One holder row: its `kind`, the typed view, its row `status`, the
/// `liveness` judged now and `safe` (true unless it still looks live: settle
/// needs no force).
fn holder_row(intent: &GcIntent, liveness: Liveness) -> SdkValue {
    let holder = &MountHolderView::of(intent);
    let text = |value: Option<&str>| SdkValue::opt_text(value);
    let workspace = holder.workspace.map(|ws| hex(&ws));
    SdkValue::map([
        ("operation", SdkValue::text(hex(&holder.op))),
        ("kind", SdkValue::text(intent.kind.as_str())),
        ("branch", text(holder.branch.as_deref())),
        ("project", text(holder.project.as_deref())),
        ("workspace", text(workspace.as_deref())),
        ("sandbox", text(holder.sandbox.as_deref())),
        ("machine", text(holder.machine.as_deref())),
        ("owner_runtime", text(holder.owner_runtime.as_deref())),
        ("owner_boot", text(holder.owner_boot.as_deref())),
        (
            "machine_incarnation",
            text(holder.machine_incarnation.as_deref()),
        ),
        (
            "sandbox_generation",
            text(holder.sandbox_generation.as_deref()),
        ),
        ("status", SdkValue::text(holder.state())),
        ("liveness", SdkValue::text(liveness_word(liveness))),
        ("safe", SdkValue::Bool(liveness != Liveness::Live)),
        ("legacy", SdkValue::Bool(legacy(intent))),
    ])
}

/// The operator label a release records. An absent label is the local
/// operator; a label is at most 128 characters without control characters.
/// It is not an authenticated principal, only the name the caller gives.
fn operator_label(operator: &str) -> Result<&str> {
    let operator = operator.trim();
    if operator.is_empty() {
        return Ok("local");
    }
    if operator.len() > 128 || operator.chars().any(char::is_control) {
        return Err(refused(
            "InvalidInput",
            "operator must be at most 128 characters without control characters",
        ));
    }
    Ok(operator)
}

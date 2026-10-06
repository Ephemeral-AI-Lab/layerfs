//! Independent small-fixture oracle over public consumer and owner operations.
use layerfs_content::{
    filesystem::{attributes::PortableMetadata, PathName},
    object::inode_leaf::InodeKind,
    AuthenticatedObjects, ContentError, ObjectId,
};
use layerfs_daemon::{
    upstream::{OperationRefusal, Upstream, UpstreamOperation},
    Command, Completion, OwnerClient, OwnerError, Pending, Response,
};
use layerfs_overlay::{BaseSource, Route};
use layerfs_sdk::client::{Calls, ClientRequest, FailureOrigin, ReplyView};
use layerfs_workspace::{Operation, Outcome, Position, SourceView, Time, ViewStat};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

#[path = "../../tests/support/upstream_payload.rs"]
mod payload;
use payload::{FILES, RAW_TARGET};

type ProofResult<T> = Result<T, Box<dyn Error>>;
const DIRECTORIES: &[&str] = &[".", ".git", ".cache", ".cache/dependency", "output"];
const NOW: Time = Time {
    seconds: -7,
    nanoseconds: 42,
};

/// Retains every exact operation and any unreleased source on a failed proof.
pub struct Failure {
    cause: Box<dyn Error>,
    operations: Vec<UpstreamOperation>,
    sources: Vec<BaseSource>,
}
impl fmt::Debug for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut output = f.debug_struct("OriginalProofFailure");
        output
            .field("cause", &self.cause)
            .field("sources", &self.sources);
        for operation in &self.operations {
            output.field("operation", operation);
            match operation.failure() {
                Ok(cause) => {
                    output.field("original_port_failure", &*cause);
                }
                Err(error) => {
                    output.field("failure_access", &error);
                }
            }
        }
        output.finish()
    }
}
enum CommandFailure {
    Admission(OwnerError, Command),
    Pending(Pending),
    Owner(OwnerError),
    Completion(Completion),
    Shape(Completion),
    Scope(OperationRefusal),
    Maintenance(Arc<layerfs_overlay::OverlayError>),
}
impl fmt::Debug for CommandFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Admission(error, command) => f
                .debug_tuple("Unattempted")
                .field(error)
                .field(command)
                .finish(),
            Self::Pending(pending) => {
                let _retained = pending;
                f.write_str("OriginalPendingExceeded5s")
            }
            Self::Owner(error) => fmt::Debug::fmt(error, f),
            Self::Completion(completion) => f
                .debug_tuple("OriginalCompletion")
                .field(completion.result())
                .finish(),
            Self::Shape(completion) => f
                .debug_tuple("OriginalUnexpectedResponse")
                .field(completion.result())
                .finish(),
            Self::Scope(refusal) => fmt::Debug::fmt(refusal, f),
            Self::Maintenance(error) => f
                .debug_tuple("OriginalMaintenanceFailure")
                .field(error)
                .finish(),
        }
    }
}
impl fmt::Display for CommandFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl Error for CommandFailure {}

fn check(condition: bool, message: &'static str) -> ProofResult<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn command(owner: &OwnerClient, route: Route, command: Command) -> ProofResult<Completion> {
    let pending = owner
        .try_submit(Some(route), command)
        .map_err(|(error, command)| CommandFailure::Admission(error, command))?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(completion) = pending.try_complete().map_err(CommandFailure::Owner)? {
            if completion.result().is_err() {
                return Err(Box::new(CommandFailure::Completion(completion)));
            }
            return Ok(completion);
        }
        if Instant::now() >= deadline {
            return Err(Box::new(CommandFailure::Pending(pending)));
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn done(owner: &OwnerClient, route: Route, request: Command) -> ProofResult<()> {
    let completion = command(owner, route, request)?;
    if !matches!(completion.result(), Ok(Response::Done)) {
        return Err(Box::new(CommandFailure::Shape(completion)));
    }
    Ok(())
}
struct Proof<'a> {
    upstream: &'a Upstream,
    operations: Vec<UpstreamOperation>,
    sources: Vec<BaseSource>,
    next_source: u64,
}
impl Proof<'_> {
    /// Keep the original operation before the first fallible source/provider job.
    fn step<T>(
        &mut self,
        f: impl FnOnce(&UpstreamOperation, &SourceView) -> ProofResult<T>,
    ) -> ProofResult<T> {
        let operation = self.upstream.operation().map_err(CommandFailure::Scope)?;
        self.operations.push(operation);
        let operation = self.operations.last().expect("retained original operation");
        let route = operation.workspace().route();
        let completion = command(
            operation.overlay(),
            route,
            Command::AcquireBaseSource {
                owner: self.next_source,
            },
        )?;
        self.next_source += 1;
        let source = match completion.result() {
            Ok(Response::BaseSource(source)) => *source,
            _ => return Err(Box::new(CommandFailure::Shape(completion))),
        };
        drop(completion);
        self.sources.push(source);
        let view = operation.workspace().view_for_source(source)?;
        let value = f(operation, &view)?;
        drop(view);
        done(
            operation.overlay(),
            route,
            Command::ReleaseBaseSource(source),
        )?;
        check(
            self.sources.pop() == Some(source),
            "exact original source release",
        )?;
        Ok(value)
    }
    fn resolve(&mut self, path: &str) -> ProofResult<ViewStat> {
        self.step(|operation, view| resolve(operation, view, path))
    }
    fn bytes(&mut self, serial: u64, expected: &[u8]) -> ProofResult<()> {
        self.step(|operation, view| {
            let mut bytes = Vec::new();
            let length = view.read(
                operation.overlay(),
                serial,
                0,
                expected.len() as u32 + 1,
                &mut bytes,
            )?;
            check(
                length == expected.len() as u64 && bytes == expected,
                "exact file bytes and EOF",
            )
        })
    }
    fn mutate(&mut self, operation: Operation) -> ProofResult<ViewStat> {
        self.step(|scope, view| {
            let outcome =
                scope
                    .workspace()
                    .mutate(scope.overlay(), scope.serials(), view, operation, NOW)?;
            match outcome {
                Outcome::Applied { publication, stat } => {
                    // This external consumer has received the published result;
                    // release the exact reply owner once before source release.
                    done(
                        scope.overlay(),
                        scope.workspace().route(),
                        Command::ReplyAttempted(publication),
                    )?;
                    stat.ok_or_else(|| "published mutation omitted stat".into())
                }
                Outcome::Unchanged { .. } => Err("selected mutation did not publish".into()),
            }
        })
    }
}
fn resolve(operation: &UpstreamOperation, view: &SourceView, path: &str) -> ProofResult<ViewStat> {
    let mut stat = view.stat(operation.overlay(), view.root_serial())?;
    if path != "." {
        for component in path.split('/') {
            stat = view.lookup(operation.overlay(), stat.serial, &PathName::new(component)?)?;
        }
    }
    Ok(stat)
}
fn expected_paths() -> BTreeSet<String> {
    DIRECTORIES
        .iter()
        .copied()
        .chain(FILES.iter().map(|(path, _)| *path))
        .chain(std::iter::once("raw-link"))
        .map(String::from)
        .collect()
}
fn metadata(path: &Path) -> ProofResult<BTreeMap<String, PortableMetadata>> {
    let bytes = std::fs::read(path)?;
    check(bytes.len() <= 8192, "native metadata oracle byte window")?;
    let mut output = BTreeMap::new();
    for line in std::str::from_utf8(&bytes)?.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        check(fields.len() == 4, "native metadata oracle schema")?;
        let value = PortableMetadata {
            mode: fields[1].parse()?,
            mtime_seconds: fields[2].parse()?,
            mtime_nanoseconds: fields[3].parse()?,
        };
        check(
            output.insert(fields[0].to_string(), value).is_none(),
            "duplicate native metadata oracle path",
        )?;
    }
    check(
        output.keys().cloned().collect::<BTreeSet<_>>() == expected_paths(),
        "complete independent metadata oracle paths",
    )?;
    Ok(output)
}
fn names(directory: &str) -> BTreeSet<Vec<u8>> {
    let prefix = if directory == "." {
        String::new()
    } else {
        format!("{directory}/")
    };
    expected_paths()
        .iter()
        .filter_map(|path| {
            let suffix = path.strip_prefix(&prefix)?;
            if suffix == "." || suffix.is_empty() || suffix.contains('/') {
                None
            } else {
                Some(suffix.as_bytes().to_vec())
            }
        })
        .collect()
}

fn original_failure(proof: &mut Proof<'_>, calls: &Calls) -> ProofResult<()> {
    let missing = ObjectId::for_bytes(b"r4-external-known-absent-object");
    let operation = proof.upstream.operation().map_err(CommandFailure::Scope)?;
    proof.operations.push(operation);
    let failed = proof
        .operations
        .last()
        .expect("retained failed-demand operation");
    let error = match failed.client().read_canonical(missing) {
        Err(error) => error,
        Ok(_) => return Err("independent missing-object request unexpectedly succeeded".into()),
    };
    check(
        error == ContentError::MissingObject,
        "original missing-object class",
    )?;
    let original = {
        let cause = failed.failure()?;
        let cause = cause.as_ref().ok_or("original PortFailure missing")?;
        println!("R4_ORIGINAL_PORT_FAILURE {cause:?}");
        let reply = cause.reply().ok_or("original failure reply missing")?;
        check(
            matches!(
                ReplyView::decode(reply.bytes())?,
                ReplyView::Failure {
                    origin: FailureOrigin::Dispatched,
                    ..
                }
            ),
            "real host dispatched original failure",
        )?;
        (reply.envelope(), reply.bytes().to_vec())
    };
    let second = match failed.client().read_canonical(missing) {
        Err(error) => error,
        Ok(_) => return Err("same failed provider unexpectedly succeeded".into()),
    };
    check(
        matches!(second, ContentError::ResourceUnavailable { .. }),
        "same failed operation terminal refusal",
    )?;
    {
        let cause = failed.failure()?;
        let reply = cause
            .as_ref()
            .and_then(|cause| cause.reply())
            .ok_or("retained original reply missing")?;
        check(
            reply.envelope() == original.0 && reply.bytes() == original.1,
            "unchanged original PortFailure reply",
        )?;
    }
    // Exactly one diagnostic demand on the same public Calls proves that the
    // preceding terminal read consumed no request correlation and sent no replay.
    let root = proof.upstream.expected().snapshot.effective_root;
    let reply = calls.call(ClientRequest::objects(None, &[root]))?;
    check(
        reply.envelope().correlation
            == original
                .0
                .correlation
                .checked_add(1)
                .ok_or("correlation exhausted")?,
        "failed provider sent no second exchange",
    )?;
    let values = match ReplyView::decode(reply.bytes())? {
        ReplyView::Objects(values) => values,
        _ => return Err("healthy original Calls root probe refused".into()),
    };
    check(
        values.len() == 1
            && values
                .clone()
                .all(|(id, bytes)| id == root && ObjectId::for_bytes(bytes) == root),
        "healthy same-Calls authenticated root probe",
    )?;
    Ok(())
}
fn oracle(proof: &mut Proof<'_>, expected: &BTreeMap<String, PortableMetadata>) -> ProofResult<()> {
    for path in DIRECTORIES {
        let stat = proof.resolve(path)?;
        check(stat.kind == InodeKind::Directory, "original directory kind")?;
        check(
            stat.metadata == expected[*path],
            "exact native directory portable metadata",
        )?;
        stat.metadata.validate(stat.kind)?;
        check(
            stat.namespace_refs == if *path == "." { 0 } else { 1 },
            "original directory reference count",
        )?;
        let expected_names = names(path);
        proof.step(|operation, view| {
            let mut actual = BTreeSet::new();
            let mut continuation = None;
            // This explicit fixture has at most eight names per directory. The
            // bound detects a broken continuation; it is not a product cap.
            for _ in 0..=expected_names.len() {
                let page = view.list(operation.overlay(), stat.serial, continuation.as_deref())?;
                for (name, _) in page.entries {
                    check(actual.insert(name), "duplicate original directory name")?;
                }
                let Some(next) = page.continuation else {
                    return check(
                        actual == expected_names,
                        "complete exact original directory names",
                    );
                };
                check(
                    continuation
                        .as_ref()
                        .is_none_or(|previous| previous < &next),
                    "ordered directory continuation progress",
                )?;
                continuation = Some(next);
            }
            Err("fixture directory enumeration exceeded declared name count".into())
        })?;
    }
    let mut hardlink = None;
    for (path, bytes) in FILES {
        let looked_up = proof.resolve(path)?;
        let stat =
            proof.step(|operation, view| Ok(view.stat(operation.overlay(), looked_up.serial)?))?;
        check(
            stat == looked_up && stat.kind == InodeKind::RegularFile,
            "stable original file stat",
        )?;
        check(
            stat.metadata == expected[*path],
            "exact native file portable metadata",
        )?;
        stat.metadata.validate(stat.kind)?;
        check(
            stat.logical_len == bytes.len() as u64,
            "original file length",
        )?;
        let linked = *path == "shared.bin" || *path == "shared-alias.bin";
        check(
            stat.namespace_refs == if linked { 2 } else { 1 },
            "original file namespace reference count",
        )?;
        if linked {
            if let Some(first) = &hardlink {
                check(
                    first == &stat,
                    "hardlinks share exact stable serial and metadata",
                )?;
            } else {
                hardlink = Some(stat.clone());
            }
        }
        proof.bytes(stat.serial, bytes)?;
    }
    let link = proof.resolve("raw-link")?;
    check(
        link.kind == InodeKind::Symlink
            && link.namespace_refs == 1
            && link.logical_len == RAW_TARGET.len() as u64,
        "original symlink facts",
    )?;
    check(
        link.metadata == expected["raw-link"],
        "exact native symlink portable metadata",
    )?;
    link.metadata.validate(link.kind)?;
    proof.step(|operation, view| {
        let target = view.readlink(operation.overlay(), link.serial)?;
        check(
            target.as_bytes() == RAW_TARGET,
            "exact raw symlink target bytes",
        )
    })?;
    Ok(())
}
fn local(proof: &mut Proof<'_>) -> ProofResult<()> {
    let parent = proof.upstream.expected().root_serial;
    let created = proof.mutate(Operation::Create {
        parent,
        name: PathName::new("local.bin")?,
        mode: 0o600,
    })?;
    check(
        created.kind == InodeKind::RegularFile
            && created.namespace_refs == 1
            && created.logical_len == 0,
        "local create facts",
    )?;
    let written = proof.mutate(Operation::Write {
        serial: created.serial,
        position: Position::At(0),
        data: (&b"local-data"[..]).into(),
    })?;
    check(
        written.serial == created.serial && written.logical_len == 10,
        "local write facts",
    )?;
    proof.bytes(created.serial, b"local-data")?;
    let appended = proof.mutate(Operation::Write {
        serial: created.serial,
        position: Position::End,
        data: (&b"+tail"[..]).into(),
    })?;
    check(
        appended.serial == created.serial && appended.logical_len == 15,
        "local append facts",
    )?;
    proof.bytes(created.serial, b"local-data+tail")?;
    let truncated = proof.mutate(Operation::SetAttributes {
        serial: created.serial,
        mode: None,
        mtime: None,
        size: Some(5),
    })?;
    check(
        truncated.logical_len == 5
            && truncated.metadata.mode == 0o600
            && truncated.metadata.mtime_seconds == NOW.seconds
            && truncated.metadata.mtime_nanoseconds == NOW.nanoseconds,
        "local truncate portable facts",
    )?;
    proof.bytes(created.serial, b"local")?;
    let looked_up = proof.resolve("local.bin")?;
    check(
        looked_up == truncated,
        "fresh operation sees published local inode",
    )?;
    proof.step(|operation, _| {
        check(
            operation.failure()?.is_none(),
            "fresh local operation owns no failed demand",
        )
    })?;
    Ok(())
}
pub fn run(upstream: &Upstream, calls: &Arc<Calls>, metadata_path: &Path) -> Result<(), Failure> {
    let mut proof = Proof {
        upstream,
        operations: Vec::new(),
        sources: Vec::new(),
        next_source: 1,
    };
    let result = (|| {
        let expected = metadata(metadata_path)?;
        original_failure(&mut proof, calls)?;
        oracle(&mut proof, &expected)?;
        local(&mut proof)?;
        check(
            proof.sources.is_empty(),
            "all exact processing sources released",
        )?;
        for operation in proof.operations.iter().skip(1) {
            check(
                operation.failure()?.is_none(),
                "fresh same-Calls operation retained no demand failure",
            )?;
        }
        let work = upstream.cache_work()?;
        check(
            work.charged_cache_bytes <= 8192,
            "one declared logical canonical allowance",
        )?;
        println!(
            "R4_FULL_ROOT_ORACLE_PASS files={} native_metadata={} cache={work:?}",
            FILES.len(),
            expected.len()
        );
        Ok(())
    })();
    match result {
        Ok(()) => Ok(()),
        Err(cause) => Err(Failure {
            cause,
            operations: proof.operations,
            sources: proof.sources,
        }),
    }
}
pub fn close(upstream: &Upstream) -> ProofResult<()> {
    let owner = upstream.owner();
    let route = upstream.route();
    let publications = command(&owner, route, Command::PendingPublications { after: 0 })?;
    match publications.result() {
        Ok(Response::Publications(values)) => check(
            values.is_empty(),
            "every published mutation reply owner released",
        )?,
        _ => return Err(Box::new(CommandFailure::Shape(publications))),
    }
    drop(publications);
    // MaintenanceIdle validates a live/present route; it cannot observe a
    // deleted namespace. Observe real live maintenance first, then use the
    // owning terminal observer for automatic deletion after known Close.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(error) = owner.maintenance_failure()? {
            return Err(Box::new(CommandFailure::Maintenance(error)));
        }
        let completion = command(&owner, route, Command::MaintenanceIdle)?;
        match completion.result() {
            Ok(Response::MaintenanceIdle(true)) => {
                println!("R4_AUTOMATIC_MAINTENANCE_IDLE observed=true");
                break;
            }
            Ok(Response::MaintenanceIdle(false)) => (),
            _ => return Err(Box::new(CommandFailure::Shape(completion))),
        }
        drop(completion);
        check(
            Instant::now() < deadline,
            "automatic local maintenance exceeded 5s",
        )?;
        std::thread::sleep(Duration::from_millis(1));
    }
    done(&owner, route, Command::Close)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(error) = owner.maintenance_failure()? {
            return Err(Box::new(CommandFailure::Maintenance(error)));
        }
        let completion = command(&owner, route, Command::CleanupState)?;
        match completion.result() {
            Ok(Response::CleanupState(layerfs_overlay::CleanupState::Gone)) => {
                println!("R4_AUTOMATIC_TERMINAL_CLEANUP gone=true");
                return Ok(());
            }
            Ok(Response::CleanupState(layerfs_overlay::CleanupState::Queued)) => (),
            Ok(Response::CleanupState(layerfs_overlay::CleanupState::Held)) => {
                return Err("terminal cleanup retained unexpected source/reply ownership".into());
            }
            _ => return Err(Box::new(CommandFailure::Shape(completion))),
        }
        drop(completion);
        check(
            Instant::now() < deadline,
            "automatic terminal cleanup exceeded 5s",
        )?;
        std::thread::sleep(Duration::from_millis(1));
    }
}

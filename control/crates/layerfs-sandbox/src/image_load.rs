//! Copying a Sandbox image to a machine (`sandbox.create --image`,
//! `machine.load`; R1, R3): the image named by digest, Docker Id or Docker
//! reference is resolved, and when the machine does not hold it yet, it is
//! exported from the host's Docker as one flat root filesystem tar
//! ([`crate::image_tar`], init-layer directory mtimes set to epoch 0 so exports
//! >=2 s apart are byte-identical for resume), sent in 1 MiB frames and unpacked
//! once there ([`crate::image_artifact`]). The machine is asked first, so an
//! image it holds needs neither Docker nor a byte sent. One copy per image and
//! machine at a time: the second one waits in [`ImageLock`], and a copy whose
//! owner died is taken over at once (#175 R01). Each export uses its own
//! container (`layerfs-export-<home8>-<machine>-<nonce>`) labeled with the
//! home, machine, image and nonce, and removes only that container and its
//! anonymous volumes on exit or error; residual containers of this home left
//! by killed copies can be enumerated ([`owned_containers`]) and are reclaimed
//! under [`ImageLock`] via [`cleanup_owned`] on subsequent loads in that home. Progress phases are `inspect`,
//! `lock` (`waiting: "lock"` while another copy holds it), `export`, `send`
//! (`done`/`total` bytes) and `unpack`.
use crate::deploy::Side;
use crate::docker::{self, Docker, Failure, Inspected};
use crate::image_artifact::{self as cache, CHUNK};
pub use crate::image_lock::ImageLock;
use crate::image_tar::{Filtered, TarReport};
use crate::images::{refused_with, stopped};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::home;
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_store::place::Worker;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;
use std::process::Child;
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// Docker `layerfs.role` label marking an ephemeral image-export container.
pub const EXPORT_ROLE: &str = "image-export";

/// The side an image is copied to: the image cache of `machine`, reached
/// through its worker or in this process (native Linux), and the `home` that
/// keeps the copy's lock.
pub struct Target<'a> {
    home: &'a Path,
    machine: &'a str,
    place: Place<'a>,
}

/// Where the bytes are written.
pub enum Place<'a> {
    /// This process, whose root is the home at the path.
    Local,
    /// The machine's worker.
    Worker(&'a mut Worker),
}

impl<'a> Target<'a> {
    /// A copy into this process's own image cache (`native` on Linux).
    pub fn local(home: &'a Path, machine: &'a str) -> Self {
        Target {
            home,
            machine,
            place: Place::Local,
        }
    }

    /// A copy into `machine`'s image cache through its worker.
    pub fn worker(home: &'a Path, machine: &'a str, worker: &'a mut Worker) -> Self {
        Target {
            home,
            machine,
            place: Place::Worker(worker),
        }
    }

    /// The home that keeps this copy's lock.
    pub fn home(&self) -> &Path {
        self.home
    }

    /// The machine whose image cache the copy fills.
    pub fn machine(&self) -> &str {
        self.machine
    }

    /// One `image.artifact` step there.
    fn step(&mut self, args: SdkValue) -> Outcome {
        match &mut self.place {
            Place::Worker(worker) => worker
                .attempt("image.artifact", args, &mut |_| {})
                .map_err(|unreached| stopped(unreached.0))?,
            Place::Local => cache::step(&Side::Home(self.home), &args),
        }
    }

    /// `probe` of image `id` (or of the image a sidecar names `reference`);
    /// `hash` asks for the staged bytes' sha256, which only the probe a
    /// copy resumes from needs.
    fn probe(
        &mut self,
        id: Option<&str>,
        reference: Option<&str>,
        archive: Option<&str>,
        hash: bool,
    ) -> Outcome {
        let text = |v: Option<&str>| v.map_or(SdkValue::Null, SdkValue::text);
        self.step(SdkValue::map([
            ("step", SdkValue::text("probe")),
            ("id", text(id)),
            ("ref", text(reference)),
            ("archive", text(archive)),
            ("hash", SdkValue::Bool(hash)),
        ]))
    }
}

/// An image a machine holds after [`load`].
#[derive(Clone, Debug, PartialEq)]
pub struct Loaded {
    /// Its id there: the Docker image Id, or an archive's config digest.
    pub id: String,
    /// Its references.
    pub refs: Vec<String>,
    /// The bytes sent, else the sidecar's size, else what Docker's layers
    /// unpack to when nothing was sent.
    pub size: Option<u64>,
    /// Whether the machine held it already (nothing was sent).
    pub reused: bool,
    /// What the copy left out and expanded; `None` when nothing was sent.
    pub report: Option<TarReport>,
}

impl Loaded {
    /// `machine.load`'s answer, `{id, refs, size, reused}`, with `dropped` and
    /// `sparse` when the image was sent.
    pub fn to_value(&self) -> SdkValue {
        let mut value = SdkValue::map([
            ("id", SdkValue::text(&self.id)),
            (
                "refs",
                SdkValue::List(self.refs.iter().cloned().map(SdkValue::Text).collect()),
            ),
            (
                "size",
                self.size
                    .map_or(SdkValue::Null, |n| SdkValue::Int(n as i64)),
            ),
            ("reused", SdkValue::Bool(self.reused)),
        ]);
        if let (SdkValue::Map(fields), Some(SdkValue::Map(report))) =
            (&mut value, self.report.as_ref().map(TarReport::to_value))
        {
            fields.extend(report);
        }
        value
    }
}

fn phase(events: &mut Events<'_>, name: &str, bytes: Option<(u64, Option<u64>)>) {
    let mut fields = vec![("phase".to_owned(), SdkValue::text(name))];
    if let Some((done, total)) = bytes {
        fields.push(("done".to_owned(), SdkValue::Int(done as i64)));
        if let Some(total) = total {
            fields.push(("total".to_owned(), SdkValue::Int(total as i64)));
        }
    }
    events.progress(SdkValue::Map(fields));
}

/// Makes `image` (a digest, Docker Id or Docker reference) present on the
/// machine `target` reaches, whose Sandboxes read `archive` (its own image
/// archive, else the bundle's), copying it from `docker` when needed.
pub fn load(
    docker: &Docker,
    events: &mut Events<'_>,
    target: &mut Target<'_>,
    archive: Option<&str>,
    image: &str,
) -> Result<Loaded> {
    if image.trim().is_empty() {
        return Err(refused("InvalidInput", "the image is empty"));
    }
    phase(events, "inspect", None);
    let held = |probe: &SdkValue| probe.get_bool("present");
    // the size the machine's sidecar records, as `machine.images` lists it
    let kept = |probe: &SdkValue| probe.get_int("size").map(|n| n.max(0) as u64);
    // 1. the machine holds that Id already: Docker is not needed
    if cache::digest(image) {
        let probe = target.probe(Some(image), None, archive, false)?;
        if held(&probe) {
            return Ok(reused(image, Vec::new(), kept(&probe)));
        }
    }
    // 2. Docker's answer; 3. without Docker, a cached image of that ref
    let mut inspected = match docker.inspect(&[image]) {
        Ok(mut found) if !found.is_empty() => found.remove(0),
        Ok(_) => return Err(Failure::Missing(String::new()).refusal(image)),
        Err(Failure::Missing(m)) => return Err(Failure::Missing(m).refusal(image)),
        Err(failure) => {
            if !cache::digest(image) {
                let probe = target.probe(None, Some(image), archive, false)?;
                if let (true, Some(id)) = (held(&probe), probe.get_text("id")) {
                    return Ok(reused(id, vec![image.to_owned()], kept(&probe)));
                }
            }
            return Err(failure.refusal(image));
        }
    };
    docker.measure(std::slice::from_mut(&mut inspected));
    let Inspected { id, refs, .. } = &inspected;
    let mut probe = target.probe(Some(id), None, archive, false)?;
    let machine_arch = probe.get_text("architecture").unwrap_or("").to_owned();
    if inspected.os != "linux" {
        return Err(refused_with(
            "Unsupported",
            format!("image {image} is for {}, not linux", inspected.os),
            SdkValue::map([
                ("reason", SdkValue::text("os")),
                ("value", SdkValue::text(&inspected.os)),
            ]),
        ));
    }
    if inspected.architecture != machine_arch {
        return Err(refused_with(
            "Unsupported",
            format!(
                "image {image} is for {}; this machine runs {machine_arch}",
                inspected.architecture
            ),
            SdkValue::map([
                ("reason", SdkValue::text("arch")),
                ("value", SdkValue::text(&inspected.architecture)),
            ]),
        ));
    }
    if held(&probe) {
        let size = kept(&probe).or(Some(inspected.size));
        return Ok(reused(id, refs.clone(), size));
    }
    // one copy at a time of this image on this machine (#175 R01): a second
    // copy waits here, and takes over at once from one whose owner died
    let machine = target.machine().to_owned();
    let _lock = ImageLock::take(target.home(), &machine, id, events)?;
    // under the lock: the copy that held it may have published the image
    if let Some(home_id) = existing_home_id(target.home())? {
        if let Ok(residual) = owned_for_home_id(docker, &home_id, Some(&machine), Some(id)) {
            for entry in residual {
                remove_container(docker, &entry.name);
            }
        }
        // Reclaim any other unheld residual export containers in this home
        // (such as killed loads for other images or machines) without waiting
        // on another copy's lock.
        let _ = cleanup_owned(docker, target.home(), None, None);
    }
    probe = target.probe(Some(id), None, archive, true)?;
    if held(&probe) {
        let size = kept(&probe).or(Some(inspected.size));
        return Ok(reused(id, refs.clone(), size));
    }
    // capacity before the first effect on the machine (#175 R03): no export
    // container, no staged byte and no cache folder; a resumed copy counts
    // the bytes it already staged once
    let staged = probe.get_int("offset").unwrap_or(0).max(0) as u64;
    capacity(&machine, &probe, id, inspected.size, staged)?;
    let home_id = ensure_home_id(target.home())?;
    phase(events, "export", None);
    let container = ExportContainer::create(docker, &home_id, &machine, id)?;
    let unpacked = docker
        .read(
            &[
                "container",
                "inspect",
                "--size",
                "--format",
                "{{.SizeRootFs}}",
                container.name(),
            ],
            docker::READ,
        )
        .ok()
        .and_then(|out| out.trim().parse::<u64>().ok())
        .filter(|n| *n > 0);
    // the container's root refines the progress estimate only: the capacity
    // decision above is taken before anything is created
    let estimate = Some(unpacked.unwrap_or(0).max(inspected.size)).filter(|n| *n > 0);
    let prefix = probe.get_text("prefix_sha256").map(str::to_owned);
    let (sent, sha256, report) = match send(
        docker,
        events,
        target,
        (&probe, inspected.size),
        (container.name(), id),
        (staged, prefix),
        estimate,
    )? {
        Some(done) => done,
        // the staged bytes are not this export's prefix: start over
        None => send(
            docker,
            events,
            target,
            (&probe, inspected.size),
            (container.name(), id),
            (0, None),
            estimate,
        )?
        .ok_or_else(|| refused("Failed", "the image export changed while it was sent"))?,
    };
    phase(events, "unpack", None);
    target.step(SdkValue::map([
        ("step", SdkValue::text("publish")),
        ("id", SdkValue::text(id)),
        ("size", SdkValue::Int(sent as i64)),
        ("sha256", SdkValue::text(&sha256)),
        (
            "refs",
            SdkValue::List(refs.iter().cloned().map(SdkValue::Text).collect()),
        ),
        ("architecture", SdkValue::text(&inspected.architecture)),
        ("config", inspected.config.clone()),
    ]))?;
    container.remove();
    Ok(Loaded {
        id: id.clone(),
        refs: refs.clone(),
        size: Some(sent),
        reused: false,
        report: Some(report),
    })
}

fn reused(id: &str, refs: Vec<String>, size: Option<u64>) -> Loaded {
    Loaded {
        id: id.to_owned(),
        refs,
        size,
        reused: true,
        report: None,
    }
}

/// An export container owned by a LayerFS home (`layerfs.role=image-export`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedContainer {
    /// Docker container name (`layerfs-export-<home8>-<machine>-<nonce>`).
    pub name: String,
    /// Full 32-hex home id (`layerfs.home`).
    pub home_id: String,
    /// Target machine name (`layerfs.machine`).
    pub machine: String,
    /// Docker image Id (`layerfs.image`).
    pub image: String,
    /// One-time export nonce (`layerfs.export.nonce`).
    pub nonce: String,
}

/// Outcome of recovering residual export containers owned by a home.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cleanup {
    /// Container names (with their anonymous volumes) removed under [`ImageLock`].
    pub removed: Vec<String>,
    /// Container names left alone because a live copy holds their [`ImageLock`].
    pub active: Vec<String>,
    /// Container names (or their anonymous volumes) that failed to be removed.
    pub failed: Vec<String>,
}

/// Reads `home`'s 32-hex id without creating files when `home` is empty.
fn existing_home_id(home: &Path) -> Result<Option<String>> {
    if home::guest::is_root(home) {
        return Ok(Some(home::guest::file(home)?.id));
    }
    match home::detect(home)? {
        home::Layout::Current(file) | home::Layout::Previous(file) => Ok(Some(file.id)),
        home::Layout::Empty => Ok(None),
        home::Layout::Old => Err(refused(
            "InvalidInput",
            format!(
                "{} is a LayerFS 1.1.0 home; it is converted on its first 2.0 call that writes",
                home.display()
            ),
        )),
    }
}

/// Returns `home`'s 32-hex id, creating `layerfs.json` under the home lock when
/// `home` is an empty folder (such as an isolated test home).
fn ensure_home_id(home: &Path) -> Result<String> {
    if let Some(id) = existing_home_id(home)? {
        return Ok(id);
    }
    let _guard = home::lock::take(home)?;
    Ok(home::ensure(home)?.id)
}

fn export_nonce() -> Result<String> {
    let bytes = layerfs_home::helpers::random()?;
    Ok(layerfs_bridge::sdk_values::hex(&bytes[..8]))
}

fn container_name(home_id: &str, machine: &str, nonce: &str) -> String {
    let home8 = &home_id[..8.min(home_id.len())];
    format!("layerfs-export-{home8}-{machine}-{nonce}")
}

/// Enumerates residual export containers owned by `home` (never another home's).
pub fn owned_containers(docker: &Docker, home: &Path) -> Result<Vec<OwnedContainer>> {
    let Some(home_id) = existing_home_id(home)? else {
        return Ok(Vec::new());
    };
    owned_for_home_id(docker, &home_id, None, None)
}

fn owned_for_home_id(
    docker: &Docker,
    home_id: &str,
    machine: Option<&str>,
    image: Option<&str>,
) -> Result<Vec<OwnedContainer>> {
    let role_filter = format!("label=layerfs.role={EXPORT_ROLE}");
    let home_filter = format!("label=layerfs.home={home_id}");
    let mut args = vec![
        "ps",
        "-a",
        "--filter",
        &role_filter,
        "--filter",
        &home_filter,
    ];
    let machine_filter = machine.map(|m| format!("label=layerfs.machine={m}"));
    if let Some(filter) = &machine_filter {
        args.extend(["--filter", filter.as_str()]);
    }
    let image_filter = image.map(|i| format!("label=layerfs.image={i}"));
    if let Some(filter) = &image_filter {
        args.extend(["--filter", filter.as_str()]);
    }
    let format = "{{.Names}}\t{{.Label \"layerfs.home\"}}\t{{.Label \"layerfs.machine\"}}\t{{.Label \"layerfs.image\"}}\t{{.Label \"layerfs.export.nonce\"}}";
    args.extend(["--format", format]);
    let listed = docker
        .read(&args, docker::READ)
        .map_err(|f| f.refusal(""))?;
    let prefix = format!("layerfs-export-{}-", &home_id[..8.min(home_id.len())]);
    let mut found = Vec::new();
    for line in listed.lines() {
        let mut cols = line.split('\t');
        let (Some(name), Some(l_home), Some(l_machine), Some(l_image), Some(l_nonce)) = (
            cols.next(),
            cols.next(),
            cols.next(),
            cols.next(),
            cols.next(),
        ) else {
            continue;
        };
        if l_home != home_id
            || !name.starts_with(&prefix)
            || l_machine.is_empty()
            || !cache::digest(l_image)
            || l_nonce.is_empty()
        {
            continue;
        }
        if machine.is_some_and(|m| m != l_machine) || image.is_some_and(|i| i != l_image) {
            continue;
        }
        found.push(OwnedContainer {
            name: name.to_owned(),
            home_id: l_home.to_owned(),
            machine: l_machine.to_owned(),
            image: l_image.to_owned(),
            nonce: l_nonce.to_owned(),
        });
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(found)
}

/// Removes residual export containers owned by `home` (optionally scoped to
/// `machine` and/or `image`) under I01's [`ImageLock`], skipping any whose lock
/// is currently held by a live copy.
pub fn cleanup_owned(
    docker: &Docker,
    home: &Path,
    machine: Option<&str>,
    image: Option<&str>,
) -> Result<Cleanup> {
    let Some(home_id) = existing_home_id(home)? else {
        return Ok(Cleanup::default());
    };
    let owned = owned_for_home_id(docker, &home_id, machine, image)?;
    let mut by_key: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for entry in owned {
        by_key
            .entry((entry.machine, entry.image))
            .or_default()
            .push(entry.name);
    }
    let mut out = Cleanup::default();
    for ((m, img), names) in by_key {
        match ImageLock::try_take(home, &m, &img)? {
            Some(_lock) => {
                for name in names {
                    if remove_container(docker, &name) {
                        out.removed.push(name);
                    } else {
                        out.failed.push(name);
                    }
                }
            }
            None => out.active.extend(names),
        }
    }
    Ok(out)
}

/// Refuses `Capacity` before the copy creates anything on the machine or in
/// the home (#175 R03): `needed` is what the copy still has to find room for
/// — the bytes already staged for a resumed copy are on the machine's
/// filesystem and are counted once — `free` is what it has, and `where` names
/// the filesystem that was measured.
fn capacity(machine: &str, probe: &SdkValue, id: &str, size: u64, staged: u64) -> Result<()> {
    // a machine that does not say how much room it has cannot be refused
    let Some(free) = probe.get_int("free").map(|n| n.max(0) as u64) else {
        return Ok(());
    };
    let needed = docker::need(size).saturating_sub(staged);
    if needed <= free {
        return Ok(());
    }
    let at = probe.get_text("root").unwrap_or(machine);
    let staged_note = if staged > 0 {
        format!(" ({staged} bytes are already staged and are not counted again)")
    } else {
        String::new()
    };
    let int = |n: u64| SdkValue::Int(n.min(i64::MAX as u64) as i64);
    Err(refused_with(
        "Capacity",
        format!(
            "machine {machine} needs {needed} more bytes in {at} to copy image {id}; {free} are free{staged_note}"
        ),
        SdkValue::map([
            ("needed", int(needed)),
            ("free", int(free)),
            ("where", SdkValue::text(at)),
        ]),
    ))
}

/// Ephemeral export container owned by one copy; removed (with its anonymous
/// volumes) on exit, error, or unwind.
struct ExportContainer<'a> {
    docker: &'a Docker,
    name: String,
    active: bool,
}

impl<'a> ExportContainer<'a> {
    fn create(docker: &'a Docker, home_id: &str, machine: &str, id: &str) -> Result<Self> {
        let nonce = export_nonce()?;
        let name = container_name(home_id, machine, &nonce);
        create_container(docker, &name, home_id, machine, id, &nonce)?;
        Ok(Self {
            docker,
            name,
            active: true,
        })
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn remove(mut self) {
        self.active = false;
        remove_container(self.docker, &self.name);
    }
}

impl Drop for ExportContainer<'_> {
    fn drop(&mut self) {
        if self.active {
            self.active = false;
            remove_container(self.docker, &self.name);
        }
    }
}

fn create_container(
    docker: &Docker,
    name: &str,
    home_id: &str,
    machine: &str,
    id: &str,
    nonce: &str,
) -> Result<()> {
    let role_label = format!("layerfs.role={EXPORT_ROLE}");
    let home_label = format!("layerfs.home={home_id}");
    let machine_label = format!("layerfs.machine={machine}");
    let image_label = format!("layerfs.image={id}");
    let nonce_label = format!("layerfs.export.nonce={nonce}");
    let result = docker.read(
        &[
            "create",
            "--name",
            name,
            "--label",
            &role_label,
            "--label",
            &home_label,
            "--label",
            &machine_label,
            "--label",
            &image_label,
            "--label",
            &nonce_label,
            "--network",
            "none",
            "--entrypoint",
            "/layerfs-export",
            id,
        ],
        Duration::from_secs(60),
    );
    if let Err(Failure::NotRunning(_)) = &result {
        // A timed-out `docker create` may still have created the container.
        remove_container(docker, name);
    }
    result.map(drop).map_err(|f| f.refusal(id))
}

fn is_missing_error(err: &Failure) -> bool {
    if matches!(err, Failure::Missing(_)) {
        return true;
    }
    let lower = err.message().to_lowercase();
    lower.contains("no such container") || lower.contains("no such volume")
}

fn remove_container(docker: &Docker, name: &str) -> bool {
    // Remove the created export container together with its anonymous volumes.
    let _ = docker.read(&["rm", "-v", name], docker::READ);
    // If still present (for instance with a stand-in `docker` that only handles
    // `rm -f <name>`, or a container requiring force), clean volumes then `-f`.
    match docker.read(
        &["container", "inspect", "--format", "{{.Id}}", name],
        docker::READ,
    ) {
        Err(err) if is_missing_error(&err) => return true,
        Err(_) => return false,
        Ok(_) => {}
    }
    let Some(volumes) = anonymous_volumes(docker, name) else {
        let _ = docker.read(&["rm", "-f", name], docker::READ);
        return false;
    };
    if let Err(err) = docker.read(&["rm", "-f", name], docker::READ) {
        if !is_missing_error(&err) {
            return false;
        }
    }
    let mut removed = true;
    for volume in volumes {
        if let Err(err) = docker.read(&["volume", "rm", &volume], docker::READ) {
            if !is_missing_error(&err) {
                removed = false;
            }
        }
    }
    removed
}

fn anonymous_volumes(docker: &Docker, name: &str) -> Option<Vec<String>> {
    let out = docker
        .read(
            &[
                "container",
                "inspect",
                "--format",
                "{{range .Mounts}}{{if eq .Type \"volume\"}}{{.Name}}{{\"\\n\"}}{{end}}{{end}}",
                name,
            ],
            docker::READ,
        )
        .ok()?;
    Some(
        out.lines()
            .map(str::trim)
            .filter(|v| v.len() == 64 && v.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
            .map(str::to_owned)
            .collect(),
    )
}

struct ExportChild(Option<Child>);

impl Drop for ExportChild {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Exports `container` and sends the filtered tar to the machine from byte
/// `staged` on, when the machine's staged bytes hash to `prefix` (else
/// `None`, nothing sent): the bytes in all, their sha256 and the filter's
/// report.
fn send(
    docker: &Docker,
    events: &mut Events<'_>,
    target: &mut Target<'_>,
    (probe, checked): (&SdkValue, u64),
    (container, id): (&str, &str),
    (staged, prefix): (u64, Option<String>),
    total: Option<u64>,
) -> Result<Option<(u64, String, TarReport)>> {
    let mut child = ExportChild(Some(
        docker
            .stream(&["export", container])
            .map_err(|f| f.refusal(id))?,
    ));
    let stdout = child
        .0
        .as_mut()
        .expect("export child still held")
        .stdout
        .take()
        .ok_or_else(|| refused("Failed", "docker export gave no output"))?;
    let mut tar = Filtered::new(stdout);
    let mut whole = Sha256::new();
    let mut buffer = vec![0u8; CHUNK];
    // the staged prefix: hashed, and kept only when it matches this export
    if staged > 0 {
        let mut left = staged;
        let mut head = Sha256::new();
        while left > 0 {
            let want = (left as usize).min(CHUNK);
            let n = fill(&mut tar, &mut buffer[..want])?;
            if n == 0 {
                break;
            }
            head.update(&buffer[..n]);
            whole.update(&buffer[..n]);
            left -= n as u64;
        }
        let matched =
            left == 0 && Some(layerfs_bridge::sdk_values::hex(&head.finalize())) == prefix;
        if !matched {
            return Ok(None);
        }
    }
    let mut offset = staged;
    let mut last = Instant::now();
    // `total` is an estimate: an event never shows more done than total
    let shown = |offset: u64| Some((offset, total.map(|t| t.max(offset))));
    phase(events, "send", shown(offset));
    loop {
        let n = fill(&mut tar, &mut buffer)?;
        if n == 0 {
            break;
        }
        let next = offset + n as u64;
        if next > checked {
            capacity(target.machine(), probe, id, next, staged)?;
        }
        whole.update(&buffer[..n]);
        target.step(SdkValue::map([
            ("step", SdkValue::text("write")),
            ("id", SdkValue::text(id)),
            ("offset", SdkValue::Int(offset as i64)),
            ("bytes", SdkValue::Bytes(buffer[..n].to_vec())),
        ]))?;
        offset = next;
        if last.elapsed() >= Duration::from_millis(200) {
            phase(events, "send", shown(offset));
            last = Instant::now();
        }
    }
    let status = child
        .0
        .take()
        .expect("export child still held")
        .wait()
        .map_err(failed)?;
    if !status.success() {
        return Err(refused(
            "Failed",
            format!("docker export {container} failed"),
        ));
    }
    // the last event is exact, with or without an estimate: done == total
    // == the bytes sent
    phase(events, "send", Some((offset, Some(offset))));
    Ok(Some((
        offset,
        layerfs_bridge::sdk_values::hex(&whole.finalize()),
        tar.into_report(),
    )))
}

/// Reads until `buffer` is full or the input ends.
fn fill(input: &mut impl Read, buffer: &mut [u8]) -> Result<usize> {
    let mut read = 0;
    while read < buffer.len() {
        let n = input.read(&mut buffer[read..]).map_err(failed)?;
        if n == 0 {
            break;
        }
        read += n;
    }
    Ok(read)
}

/// `machine.load(name, image)`: R1's copy without a Sandbox, `{id, refs,
/// size, reused}`.
pub fn machine_load(
    home: &Path,
    events: &mut Events<'_>,
    name: &layerfs_home::names::MachineName,
    image: &str,
) -> Outcome {
    use layerfs_store::place::{self, Executor};
    let machine = name.as_str();
    let choice = crate::deploy::choose(home, machine, None)?;
    let mut executor = place::on_machine(home, machine)?;
    if crate::images::host_mode(&executor) {
        return Err(crate::images::host_only(machine));
    }
    let docker = Docker::of(home)?;
    let archive = choice.archive.as_deref();
    let loaded = match &mut executor {
        Executor::Local => load(
            &docker,
            events,
            &mut Target::local(home, machine),
            archive,
            image,
        )?,
        Executor::Worker(worker) => load(
            &docker,
            events,
            &mut Target::worker(home, machine, worker),
            archive,
            image,
        )?,
    };
    Ok(loaded.to_value())
}

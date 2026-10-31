//! The Sandbox image as a read-only root filesystem: one archive per bundle,
//! unpacked once per config digest and shared by every Sandbox that names it.
//!
//! A namespace Sandbox's root is an overlay of two things: this unpacked image
//! (the lower, read-only layer) and the Sandbox's own `upper` and `work`
//! directories (its writable layer, deleted with it). The image is what the
//! commands see as `/`; the host's own userland is not part of that root.
//!
//! # The archive
//!
//! The bundle ships one **docker-archive** (`docker save`'s output) per
//! Sandbox image, named `<bundle>/images/<64 hex>.tar`, where `<64 hex>` is
//! `deploy.image` without its `sha256:` prefix. The bundle builder sets
//! `LAYERFS_IMAGE` (the digest) and `LAYERFS_IMAGE_ARCHIVE` (the absolute path
//! of that file) for the worker that runs this runtime.
//!
//! `docker-archive` is chosen over an OCI image layout because
//! - `deploy.image` is Docker's own local image identity on the Docker backend,
//!   and a docker-archive's `manifest.json` names its config by exactly that
//!   digest, so one value addresses the image on both backends;
//! - `docker save` is available everywhere the Docker backend already works,
//!   which is where the image comes from;
//! - its layer blobs are uncompressed tars, so unpacking needs no decompressor
//!   and this crate keeps its single dependency.
//!
//! A compressed layer blob (`podman save` and some other writers gzip them) is
//! refused before anything is written; the archive must be produced by
//! `docker save` or converted to that shape.
//!
//! # The digest
//!
//! `deploy.image` is `sha256:<64 lowercase hex>` and must be the archive's
//! config digest: the docker-archive names that blob `<64 hex>.json` or
//! `blobs/sha256/<64 hex>`, and [`archive`] compares that name with the
//! requested digest. An archive whose config digest is something else is
//! refused before anything is unpacked, so a Sandbox never runs a root other
//! than the one the deployment named. The config blob is not re-hashed here:
//! the archive arrives inside a sealed bundle that the provider copies in
//! whole, and this crate has no hashing dependency of its own.
//!
//! # One unpack per digest
//!
//! The unpacked image lives under `<LAYERFS_LOCAL_DIR>/images/<64 hex>`, named
//! by its digest only, so two Sandboxes that name one image share one copy.
//! The first Sandbox to need it takes a per-digest lock
//! (`images/<64 hex>.lock`, `flock`), unpacks into `images/.tmp-<64 hex>-…`
//! and renames that directory into place once it is complete, so a second
//! Sandbox either waits for that rename or finds the finished directory, and a
//! crashed unpack never leaves a half image under the real name. The lock file
//! stays: another process may still hold it.
//!
//! A command never gets write access to the image. The unpacked directory is
//! outside the Sandbox's mount tree — only the image's *contents* are composed
//! into its root, by the kernel, as a read-only lower layer — so no path a
//! command can name reaches the cache, and the cache directory itself is
//! root-owned and not writable by the command identity.
//!
//! # Extraction safety
//!
//! Every layer is applied in order, with the archive's own whiteouts
//! (`.wh.<name>` removes that name, `.wh..wh..opq` empties its directory).
//! Entries are refused, not repaired, when they are
//! - an absolute path or one with a `.`/`..` component,
//! - reachable only through a symbolic link or a non-directory (so a symlink
//!   in an early entry cannot redirect a later entry's write outside the root),
//! - a device node, a FIFO or any other entry type this runtime does not
//!   extract (regular files, directories, symlinks and hard links only).
//!
//! File ownership becomes root (the archive's numeric owners are not applied,
//! and a rootfs layer is root's), permission bits are applied as `0o777` —
//! setuid, setgid, sticky and xattrs (and so file capabilities) are not
//! restored — and a command runs with no-new-privs anyway.
//!
//! The pieces are the cache ([`cache`]), one archive's own reading
//! ([`archive`]), the tar format ([`tar`]), what one layer does to the tree
//! ([`layers`]) and the types they share ([`types`]).
mod archive;
mod cache;
mod layers;
mod tar;
mod types;

pub use cache::{cache, ensure, lookup};
pub use types::ImageRoot;
pub(crate) use types::{refuse, PATH_MAX};

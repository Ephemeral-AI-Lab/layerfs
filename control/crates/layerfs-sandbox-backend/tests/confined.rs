//! The confined open, `stat` and mount table an inspection read is made of
//! (layerfs-project#178 security rules 1 to 3 and 8), on this platform:
//! `openat2` on Linux, a walk of `O_NOFOLLOW` opens on macOS. A path stays
//! beneath its base; no link is followed, whether it leads out or not; a
//! FIFO is never waited on; only a regular file or a folder is answered.
//! And the whole-root view (step 4): links resolve inside the base, the
//! last name is never followed, and an open ends only in a mount its
//! caller allowed by id, with the mount table given as text.
#![cfg(any(target_os = "linux", target_os = "macos"))]
use layerfs_sandbox_backend::shared::confined::{self, Base, Kind, Refused, View};
use std::ffi::OsStr;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

static NEXT: AtomicU32 = AtomicU32::new(0);

fn scratch(tag: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "confined-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn open(base: &Base, relative: &str) -> Result<confined::Opened, Refused> {
    base.open_at(Path::new(relative))
}

fn names(folder: &confined::Opened) -> Vec<String> {
    let mut found = Vec::new();
    folder
        .names(&mut |name| {
            found.push(String::from_utf8_lossy(name).into_owned());
            true
        })
        .unwrap();
    found.sort();
    found
}

#[test]
fn an_open_stays_beneath_its_base_and_follows_no_link() {
    let dir = scratch("beneath");
    let base_dir = dir.join("base");
    std::fs::create_dir_all(base_dir.join("a/b")).unwrap();
    std::fs::write(base_dir.join("a/b/file"), "inside").unwrap();
    std::fs::write(dir.join("outside"), "outside").unwrap();
    // links: out of the base, and to a folder and a file inside it
    symlink(&dir, base_dir.join("out")).unwrap();
    symlink(base_dir.join("a"), base_dir.join("in")).unwrap();
    symlink("file", base_dir.join("a/b/link")).unwrap();
    let base = Base::open(&base_dir, View::Beneath).unwrap();

    let file = open(&base, "a/b/file").unwrap();
    assert_eq!((file.stat.kind, file.stat.size), (Kind::File, 6));
    let mut bytes = [0u8; 4];
    assert_eq!(file.read_at(&mut bytes, 2).unwrap(), 4);
    assert_eq!(&bytes, b"side");
    let folder = open(&base, "a/b").unwrap();
    assert_eq!(folder.stat.kind, Kind::Folder);
    assert_eq!(names(&folder), ["file", "link"]);
    // read again from the start, not from where the first reading ended
    assert_eq!(names(&folder), ["file", "link"]);
    assert_eq!(names(&open(&base, "").unwrap()), ["a", "in", "out"]);
    assert_eq!(open(&base, "./a/./b").unwrap().stat.kind, Kind::Folder);

    // a link is refused on the way and at the end, wherever it leads
    for path in ["out", "out/outside", "in", "in/b/file", "a/b/link"] {
        assert!(
            matches!(open(&base, path), Err(Refused::Symlink)),
            "{path}: {:?}",
            open(&base, path).map(|_| ())
        );
    }
    // a `..` or a root is refused before any system call
    for path in ["..", "a/../../outside", "a/..", "/etc"] {
        assert!(matches!(open(&base, path), Err(Refused::Outside)), "{path}");
    }
    for path in ["nothing", "a/nothing/x", "a/b/file/x"] {
        assert!(matches!(open(&base, path), Err(Refused::Missing)), "{path}");
    }

    // the entry itself is `stat`ed, never what a link names
    let link = folder.stat_of(OsStr::new("link")).unwrap();
    assert_eq!(link.kind, Kind::Symlink);
    assert_eq!(folder.target_of(OsStr::new("link")).unwrap(), "file");
    let root = open(&base, "").unwrap();
    assert_eq!(root.stat_of(OsStr::new("out")).unwrap().kind, Kind::Symlink);
    let stat = folder.stat_of(OsStr::new("file")).unwrap();
    assert_eq!((stat.kind, stat.size, stat.links), (Kind::File, 6, 1));
    assert_eq!(stat.device, folder.stat.device);
    assert!(stat.mtime_ms > 1_600_000_000_000, "{}", stat.mtime_ms);
    std::fs::set_permissions(base_dir.join("a/b/file"), PermissionsExt::from_mode(0o640)).unwrap();
    assert_eq!(folder.stat_of(OsStr::new("file")).unwrap().mode, 0o640);
    assert!(matches!(
        folder.stat_of(OsStr::new("nothing")),
        Err(Refused::Missing)
    ));
    // the base's real path is its descriptor's
    assert_eq!(base.path().unwrap(), base_dir.canonicalize().unwrap());
    // a base is a folder, and not a link to one
    assert!(Base::open(&base_dir.join("a/b/file"), View::Beneath).is_err());
    assert!(Base::open(&base_dir.join("in"), View::Beneath).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_special_file_is_refused_and_a_fifo_is_never_waited_on() {
    let dir = scratch("special");
    let made = std::process::Command::new("mkfifo")
        .arg(dir.join("pipe"))
        .status()
        .unwrap();
    assert!(made.success());
    let base = Base::open(&dir, View::Beneath).unwrap();
    let at = Instant::now();
    assert!(matches!(
        open(&base, "pipe"),
        Err(Refused::Special(Kind::Fifo))
    ));
    // nobody writes to it: a blocking open would wait for ever
    assert!(at.elapsed() < Duration::from_secs(2), "{:?}", at.elapsed());
    let folder = open(&base, "").unwrap();
    assert_eq!(folder.stat_of(OsStr::new("pipe")).unwrap().kind, Kind::Fifo);

    // a device, where this user may make one
    let node = dir.join("null");
    let made = std::process::Command::new("mknod")
        .arg(&node)
        .args(["c", "1", "3"])
        .output();
    match made {
        Ok(out) if out.status.success() => {
            assert_eq!(
                folder.stat_of(OsStr::new("null")).unwrap().kind,
                Kind::Device
            );
            assert!(matches!(
                open(&base, "null"),
                Err(Refused::Special(Kind::Device))
            ));
            println!("device: refused");
        }
        other => println!("NOT_RUN device: mknod did not make one here ({other:?})"),
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn the_mount_table_names_mounts_and_an_open_never_crosses_onto_one() {
    let table = confined::mounts().unwrap();
    assert!(
        table.iter().any(|mount| mount.point == Path::new("/")),
        "{table:?}"
    );
    let proc = table.iter().find(|mount| mount.point == Path::new("/proc"));
    assert_eq!(proc.map(|mount| mount.fs.as_str()), Some("proc"));

    // a mount every Linux has: `/proc` is one name below `/` and on
    // another device, so the open is refused on the way and at the end
    let root = Base::open(Path::new("/"), View::Beneath).unwrap();
    for path in ["proc", "proc/version", "proc/self/status"] {
        assert!(matches!(open(&root, path), Err(Refused::Outside)), "{path}");
    }
    let listed = open(&root, "").unwrap();
    assert_eq!(listed.stat.kind, Kind::Folder);
    // its `stat` as an entry is the mounted root's: on another device
    let entry = listed.stat_of(OsStr::new("proc")).unwrap();
    assert_ne!(entry.device, listed.stat.device);

    // a mount beneath the base, where this process may make one
    let dir = scratch("xdev");
    std::fs::create_dir_all(dir.join("on/mnt")).unwrap();
    let point = dir.join("on/mnt");
    let mounted = std::process::Command::new("mount")
        .args(["-t", "tmpfs", "tmpfs"])
        .arg(&point)
        .output();
    match mounted {
        Ok(out) if out.status.success() => {
            std::fs::write(point.join("file"), "x").unwrap();
            let base = Base::open(&dir, View::Beneath).unwrap();
            let found = confined::mounts().unwrap();
            let real = base.path().unwrap().join("on/mnt");
            let listed = found.iter().find(|mount| mount.point == real);
            assert_eq!(listed.map(|mount| mount.fs.as_str()), Some("tmpfs"));
            for path in ["on/mnt", "on/mnt/file"] {
                assert!(matches!(open(&base, path), Err(Refused::Outside)), "{path}");
            }
            assert_eq!(open(&base, "on").unwrap().stat.kind, Kind::Folder);
            let off = std::process::Command::new("umount").arg(&point).status();
            assert!(off.unwrap().success());
            println!("cross-device: refused");
        }
        other => println!(
            "NOT_RUN cross-device: no mount can be made here ({})",
            match other {
                Ok(out) => String::from_utf8_lossy(&out.stderr).trim().to_owned(),
                Err(error) => error.to_string(),
            }
        ),
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn macos_reads_its_mount_table_from_one_bounded_run_of_mount() {
    // `/sbin/mount`, given half a second: `getfsstat(MNT_NOWAIT)` has no
    // safe wrapper in this crate's dependencies
    let started = Instant::now();
    let table = confined::mounts().unwrap();
    assert!(started.elapsed() < Duration::from_secs(2));
    let fs_at = |point: &str| {
        let found = table.iter().find(|mount| mount.point == Path::new(point));
        found.map(|mount| mount.fs.clone())
    };
    assert_eq!(fs_at("/").as_deref(), Some("apfs"), "{table:?}");
    assert_eq!(fs_at("/dev").as_deref(), Some("devfs"), "{table:?}");
    assert!(table.iter().all(|mount| mount.id.is_none()));

    // the lines `mount` prints, one mount each
    let said = "/dev/disk3s1s1 on / (apfs, sealed, local, read-only, journaled)\n\
        devfs on /dev (devfs, local, nobrowse)\n\
        map auto_home on /System/Volumes/Data/home (autofs, automounted, nobrowse)\n\
        localhost:/x on /Users/me/a b/workspaces/1 (nfs, nodev, nosuid, mounted by me)\n\
        not a mount line\n";
    let parsed: Vec<(String, String)> = (confined::parse_mounts(said).into_iter())
        .map(|mount| (mount.point.display().to_string(), mount.fs))
        .collect();
    let expected = [
        ("/", "apfs"),
        ("/dev", "devfs"),
        ("/System/Volumes/Data/home", "autofs"),
        ("/Users/me/a b/workspaces/1", "nfs"),
    ];
    assert_eq!(
        parsed,
        expected.map(|(point, fs)| (point.to_owned(), fs.to_owned()))
    );
}

#[cfg(target_os = "macos")]
#[test]
fn macos_opens_a_whole_root_view_as_it_opens_beneath() {
    // no link is resolved here in either view, and `enter` wants a folder
    let dir = scratch("inroot");
    std::fs::create_dir_all(dir.join("usr/lib")).unwrap();
    std::fs::write(dir.join("usr/lib/x"), "lib").unwrap();
    symlink("usr/lib", dir.join("lib")).unwrap();
    let base = Base::open(&dir, View::InRoot).unwrap();
    assert_eq!(base.mount(), None);
    assert_eq!(open(&base, "usr/lib/x").unwrap().stat.size, 3);
    assert!(matches!(open(&base, "lib/x"), Err(Refused::Symlink)));
    assert!(matches!(
        base.enter(Path::new("lib")),
        Err(Refused::Symlink)
    ));
    assert_eq!(
        base.enter(Path::new("usr/lib")).unwrap().stat.kind,
        Kind::Folder
    );
    assert!(matches!(
        base.enter(Path::new("usr/lib/x")),
        Err(Refused::Missing)
    ));
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn a_root_view_resolves_links_inside_its_base_and_never_follows_the_last_name() {
    let dir = scratch("inroot");
    let root = dir.join("root");
    std::fs::create_dir_all(root.join("usr/lib")).unwrap();
    std::fs::write(root.join("usr/lib/x"), "lib").unwrap();
    std::fs::write(dir.join("outside"), "outside").unwrap();
    // a merged-/usr image's links, relative and absolute; a link that
    // climbs out, and one to a file
    symlink("usr/lib", root.join("lib")).unwrap();
    symlink("/usr/lib", root.join("lib64")).unwrap();
    symlink("../../outside", root.join("usr/climb")).unwrap();
    symlink(dir.join("outside"), root.join("abs")).unwrap();
    symlink("x", root.join("usr/lib/y")).unwrap();

    let base = Base::open(&root, View::InRoot).unwrap();
    assert!(base.mount().is_some());
    for path in ["usr/lib/x", "lib/x", "lib64/x"] {
        let file = open(&base, path).unwrap();
        assert_eq!((file.stat.kind, file.stat.size), (Kind::File, 3), "{path}");
        assert_eq!(file.stat.mount, base.mount());
    }
    // the last name is never followed, whatever it names
    for path in ["lib", "lib64", "usr/lib/y", "lib/y", "abs", "usr/climb"] {
        assert!(matches!(open(&base, path), Err(Refused::Symlink)), "{path}");
    }
    // a folder a path leads through is entered through its link
    for path in ["lib", "lib64", "usr/lib", ""] {
        let folder = base.enter(Path::new(path)).unwrap();
        assert_eq!(folder.stat.kind, Kind::Folder, "{path}");
    }
    assert_eq!(names(&base.enter(Path::new("lib")).unwrap()), ["x", "y"]);
    assert!(matches!(
        base.enter(Path::new("usr/lib/x")),
        Err(Refused::Missing)
    ));
    // a link out resolves inside the base, where there is nothing: the
    // base is the root of every path
    for path in ["usr/climb", "abs"] {
        let entered = base.enter(Path::new(path));
        assert!(matches!(entered, Err(Refused::Missing)), "{path}");
    }
    assert!(matches!(open(&base, "../outside"), Err(Refused::Outside)));
    // the same tree beneath: no link at all
    let beneath = Base::open(&root, View::Beneath).unwrap();
    for path in ["lib/x", "lib64/x"] {
        assert!(
            matches!(open(&beneath, path), Err(Refused::Symlink)),
            "{path}"
        );
    }
    assert!(matches!(
        beneath.enter(Path::new("lib")),
        Err(Refused::Symlink)
    ));
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn a_root_view_ends_only_in_the_mounts_its_caller_allowed_by_id() {
    // `/proc` is a mount every Linux has, of another id than `/`
    let root = Base::open(Path::new("/"), View::InRoot).unwrap();
    let slash = open(&root, "").unwrap();
    let own = root.mount().unwrap();
    assert_eq!(slash.stat.mount, Some(own));
    // an entry's `statx` names the mount at it, with no open of it
    let proc = slash.stat_of(OsStr::new("proc")).unwrap();
    let proc_id = proc.mount.unwrap();
    assert_ne!(proc_id, own);
    assert!(slash.stat.apart_from(&proc));
    assert!(!slash
        .stat
        .apart_from(&slash.stat_of(OsStr::new("etc")).unwrap()));

    // the kernel's table names that id at `/proc`
    let table = confined::mounts().unwrap();
    let named = (table.iter().rev()).find(|mount| mount.point == Path::new("/proc"));
    assert_eq!(named.and_then(|mount| mount.id), Some(proc_id), "{table:?}");

    // not allowed: refused, directly and through a link that leads there
    let dir = scratch("mountid");
    symlink("/proc", dir.join("link")).unwrap();
    let through = dir.strip_prefix("/").unwrap().join("link");
    assert!(matches!(open(&root, "proc/version"), Err(Refused::Mount)));
    assert!(matches!(open(&root, "proc"), Err(Refused::Mount)));
    assert!(matches!(root.enter(&through), Err(Refused::Mount)));
    assert!(matches!(
        root.open_at(&through.join("version")),
        Err(Refused::Mount)
    ));

    // a mount table given as text: only the mounts it names are allowed
    let line = |id: u64, point: &str, fs: &str| {
        format!("{id} 1 0:22 / {point} rw,nosuid shared:5 - {fs} {fs} rw\n")
    };
    let other = confined::parse_mounts(&line(proc_id + 1_000_000, "/proc", "proc"));
    let mut closed = Base::open(Path::new("/"), View::InRoot).unwrap();
    closed.cross(other.iter().filter_map(|mount| mount.id));
    assert!(matches!(open(&closed, "proc/version"), Err(Refused::Mount)));
    let text = format!(
        "{}{}",
        line(own, "/", "overlay"),
        line(proc_id, "/a\\040b/proc", "proc")
    );
    let parsed = confined::parse_mounts(&text);
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[1].point, Path::new("/a b/proc"));
    assert_eq!(
        (parsed[1].id, parsed[1].fs.as_str()),
        (Some(proc_id), "proc")
    );
    let mut allowed = Base::open(Path::new("/"), View::InRoot).unwrap();
    allowed.cross(parsed.iter().filter_map(|mount| mount.id));
    let version = open(&allowed, "proc/version").unwrap();
    assert_eq!(version.stat.mount, Some(proc_id));
    assert_eq!(
        allowed.open_at(&through.join("version")).unwrap().stat.kind,
        Kind::File
    );
    // and never a magic link, allowed mount or not
    assert!(open(&allowed, "proc/self/root/etc").is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn what_the_system_refuses_to_open_is_denied_not_a_failure() {
    let dir = scratch("denied");
    std::fs::create_dir(dir.join("closed")).unwrap();
    std::fs::write(dir.join("closed/inside"), "x").unwrap();
    std::fs::write(dir.join("file"), "x").unwrap();
    let close = |name: &str| {
        std::fs::set_permissions(dir.join(name), PermissionsExt::from_mode(0o000)).unwrap();
    };
    close("file");
    close("closed");
    if std::fs::File::open(dir.join("file")).is_ok() {
        println!("NOT_RUN denied opens: this user (root) opens a file of mode 000");
    } else {
        let base = Base::open(&dir, View::Beneath).unwrap();
        for path in ["file", "closed", "closed/inside"] {
            assert!(matches!(open(&base, path), Err(Refused::Denied)), "{path}");
        }
    }
    std::fs::set_permissions(dir.join("closed"), PermissionsExt::from_mode(0o700)).unwrap();
}

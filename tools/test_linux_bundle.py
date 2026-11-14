"""The sealed Linux bundle builder (#533 O16): per-package steps, the source check
before sealing, bundle.json and placement. Docker and Cargo are stubbed."""

import gzip
import hashlib
import io
import json
import subprocess
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import linux_bundle as bundle
from cleanup_state import Refused

CONFIG = b'{"architecture":"arm64","os":"linux"}'
LDD = """\tlinux-vdso.so.1 (0x0000ffff925ed000)
\tlibssl.so.3 => /lib/aarch64-linux-gnu/libssl.so.3 (0x0000ffff91a00000)
\tlibc.so.6 => /lib/aarch64-linux-gnu/libc.so.6 (0x0000ffff91310000)
\t/lib/ld-linux-aarch64.so.1 (0x0000ffff925a0000)
"""
LIBRARIES = ("libssl.so.3", "libc.so.6", "ld-linux-aarch64.so.1")


def elf(interpreter=None):
    """A minimal 64-bit little-endian ELF header with one program header: a
    `PT_INTERP` naming `interpreter`, else a `PT_LOAD` (a static executable)."""
    head = bytearray(64)
    head[:6] = b"\x7fELF\x02\x01"
    head[32:40] = (64).to_bytes(8, "little")
    head[54:56] = (56).to_bytes(2, "little")
    head[56:58] = (1).to_bytes(2, "little")
    entry = bytearray(56)
    path = (interpreter or "").encode() + b"\0"
    entry[0:4] = (3 if interpreter else 1).to_bytes(4, "little")
    entry[8:16] = (120).to_bytes(8, "little")
    entry[32:40] = len(path).to_bytes(8, "little")
    return bytes(head + entry + path)


CONFIG_HEX = hashlib.sha256(CONFIG).hexdigest()


def layer_tar():
    """One small layer: a tar holding /hello."""
    data = io.BytesIO()
    with tarfile.open(fileobj=data, mode="w", format=tarfile.USTAR_FORMAT) as tar:
        info = tarfile.TarInfo("hello")
        info.size = 5
        tar.addfile(info, io.BytesIO(b"hello"))
    return data.getvalue()


def saved_archive(path):
    """What `docker save` writes with the containerd store: OCI blobs, one
    gzip layer and one empty layer (1024 zero bytes, gzipped)."""
    blobs = {
        CONFIG_HEX: CONFIG,
        "aa" * 32: gzip.compress(layer_tar()),
        "bb" * 32: gzip.compress(b"\0" * 1024),
    }
    listing = [
        {
            "Config": f"blobs/sha256/{CONFIG_HEX}",
            "RepoTags": [],
            "Layers": [f"blobs/sha256/{h}" for h in ("aa" * 32, "bb" * 32)],
        }
    ]
    with tarfile.open(path, "w") as tar:
        for name, data in [
            *((f"blobs/sha256/{h}", b) for h, b in blobs.items()),
            ("manifest.json", json.dumps(listing).encode()),
        ]:
            info = tarfile.TarInfo(name)
            info.size = len(data)
            tar.addfile(info, io.BytesIO(data))
    path.chmod(0o600)


class LinuxBundleTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name).resolve()
        self.worktree = self.root / "checkout"
        self.worktree.mkdir()
        subprocess.run(["git", "init", "-q", str(self.worktree)], check=True)
        (self.worktree / "Cargo.toml").write_text('[workspace.package]\nversion = "9.8.7"\n')
        self.state = self.root / "state"
        (self.state / "leases").mkdir(parents=True)
        (self.state / "builds").mkdir()
        self.record = {
            "state": str(self.state),
            "worktree": str(self.worktree),
            "images": {"build": "sha256:b", "sandbox": "sha256:s"},
        }
        self.source = {"head": "abc", "sha256": "src1", "files": {}}
        self.commands = []
        protocol = self.worktree / "control/crates/layerfs-machine/src/worker/hello.rs"
        protocol.parent.mkdir(parents=True)
        protocol.write_text("pub const PROTOCOL: i64 = 3;\n")
        patches = [
            patch.object(bundle, "inputs", lambda record: dict(self.source)),
            patch.object(
                bundle,
                "docker_build",
                lambda record, command, extra=(), network="none": ["docker-run", *extra, *command],
            ),
            patch.object(bundle, "execute", self.execute),
            patch.object(bundle, "arch", lambda image: "aarch64"),
        ]
        for p in patches:
            p.start()
            self.addCleanup(p.stop)

        self.ldd = LDD
        self.copied = LIBRARIES

    def fetched(self):
        """What the `runtime` step leaves in the registration's state."""
        out = bundle.runtime_dir(self.record)
        out.mkdir(parents=True)
        files = {}
        for name in bundle.STATIC:
            (out / name).write_bytes(elf())
            files[name] = hashlib.sha256(elf()).hexdigest()
        (out / "bash-static.copyright").write_text("GPL-3+")
        packages = {"bash-static": {"version": "5.2", "deb": "d1"}, "busybox-static": {"version": "1.35", "deb": "d2"}}
        (out / "runtime.json").write_text(json.dumps({"packages": packages, "files": files}))

    def execute(self, command, cwd, env=None, timeout=None, log=None):
        self.commands.append((command, timeout))
        if command[:1] == ["docker-run"] and "sh" in command:
            export = Path(command[command.index("--mount") + 1].split("src=")[1].split(",")[0])
            if bundle.COLLECT in command:
                for directory in ("runtime/lib", "runtime/bin", "licenses"):
                    (export / directory).mkdir(parents=True, exist_ok=True)
                for name in self.copied:
                    (export / "runtime/lib" / name).write_bytes(name.encode())
                (export / "runtime/bin/getent").write_bytes(b"getent")
                (export / "licenses/libc6.copyright").write_text("LGPL-2.1+")
                (export / ".ldd").write_text(self.ldd)
                (export / ".glibc").write_text("2.36\n")
                (export / ".owners").write_text("libc6\n")
                return
            for name in bundle.BINARIES:
                (export / name).write_bytes(name.encode())
        if command[:2] == ["docker", "save"]:
            saved_archive(Path(command[3]))

    def test_each_package_is_its_own_bounded_step(self):
        for step in ("daemon", "worker", "helper"):
            bundle.build(self.record, step, "debug", 120)
        cargo = [c for c, _ in self.commands]
        self.assertEqual(
            [c[c.index("-p") + 1] for c in cargo], ["layerfs-daemon", "layerfs-cli", "layerfs-sandbox-backend"]
        )
        self.assertIn("layerfs-daemon/postgresql", cargo[0])
        self.assertIn("--no-default-features", cargo[1])
        self.assertTrue(all(t == 120 for _, t in self.commands))
        self.assertTrue(all(c[c.index("-j") + 1] == "6" for c in cargo))
        steps = json.loads((self.state / "linux-bundle/debug.json").read_text())
        self.assertEqual(steps["worker"]["source"], {"head": "abc", "sha256": "src1"})

    def test_quiet_in_the_main_checkout_means_two_jobs(self):
        (self.worktree / "QUIET").write_text("")
        self.assertEqual(bundle.jobs(self.worktree), 2)
        (self.worktree / "QUIET").unlink()
        self.assertEqual(bundle.jobs(self.worktree), 6)

    def test_seal_refuses_missing_or_stale_steps(self):
        dest = self.root / "cache"
        bundle.build(self.record, "daemon", "debug", 120)
        with self.assertRaisesRegex(Refused, "worker, helper"):
            bundle.seal(self.record, "debug", None, dest, 120)
        bundle.build(self.record, "worker", "debug", 120)
        bundle.build(self.record, "helper", "debug", 120)
        self.source = {"head": "abc", "sha256": "src2", "files": {}}
        with self.assertRaisesRegex(Refused, "daemon, worker, helper"):
            bundle.seal(self.record, "debug", None, dest, 120)
        self.assertFalse(dest.exists() and any(dest.rglob("*")), "nothing placed")

    def test_seal_places_the_bundle_and_replaces_an_old_one(self):
        dest = self.root / "cache"
        for step in ("daemon", "worker", "helper"):
            bundle.build(self.record, step, "debug", 120)
        self.fetched()
        old = dest / "9.8.7/linux-aarch64"
        old.mkdir(parents=True)
        (old / "stale").write_text("x")
        final = bundle.seal(self.record, "debug", None, dest, 120)
        self.assertEqual(final, old)
        names = sorted(p.name for p in final.iterdir())
        self.assertEqual(names, sorted([*bundle.BINARIES, bundle.ARCHIVE, "bundle.json", "licenses", "runtime"]))
        manifest = json.loads((final / "bundle.json").read_text())
        files = {n: hashlib.sha256(n.encode()).hexdigest() for n in bundle.BINARIES}
        files[bundle.ARCHIVE] = hashlib.sha256((final / bundle.ARCHIVE).read_bytes()).hexdigest()
        layer = {f"runtime/lib/{n}": hashlib.sha256(n.encode()).hexdigest() for n in LIBRARIES}
        layer["runtime/bin/getent"] = hashlib.sha256(b"getent").hexdigest()
        for name in bundle.STATIC:
            layer[f"runtime/bin/{name}"] = hashlib.sha256(elf()).hexdigest()
        files.update(layer)
        files["licenses/libc6.copyright"] = hashlib.sha256(b"LGPL-2.1+").hexdigest()
        files["licenses/bash-static.copyright"] = hashlib.sha256(b"GPL-3+").hexdigest()
        self.assertEqual(manifest["files"], files)
        # R5: the runtime layer, its own hash, loader and glibc
        self.assertEqual(
            manifest["sandbox_runtime"],
            {
                "hash": bundle.bundle_hash(layer),
                "loader": "lib/ld-linux-aarch64.so.1",
                "glibc": "2.36",
                "packages": {"bash-static": "5.2", "busybox-static": "1.35"},
            },
        )
        self.assertEqual((final / "runtime/bin/bash").stat().st_mode & 0o777, 0o755)
        self.assertEqual((final / "runtime/lib/libc.so.6").stat().st_mode & 0o777, 0o644)
        self.assertEqual((final / "runtime/lib/ld-linux-aarch64.so.1").stat().st_mode & 0o777, 0o755)
        self.assertEqual(sorted(p.name for p in final.iterdir() if p.name.startswith(".")), [])
        self.assertEqual(manifest["protocol"], 3)
        self.assertEqual(manifest["runtime"], bundle.bundle_hash({n: files[n] for n in bundle.BINARIES}))
        # the namespaces backend's shape: uncompressed layers, config named by its digest
        with tarfile.open(final / bundle.ARCHIVE) as tar:
            listing = json.load(tar.extractfile("manifest.json"))
            self.assertEqual(
                listing, [{"Config": CONFIG_HEX + ".json", "RepoTags": None, "Layers": ["0/layer.tar", "1/layer.tar"]}]
            )
            self.assertEqual(tar.extractfile(CONFIG_HEX + ".json").read(), CONFIG)
            self.assertEqual(tar.extractfile("0/layer.tar").read(), layer_tar())
            self.assertEqual(tar.extractfile("1/layer.tar").read(), b"\0" * 1024)
        self.assertEqual(manifest["image"], "sha256:" + CONFIG_HEX)
        self.assertEqual(manifest["hash"], hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest())
        self.assertEqual((manifest["saved_from"], manifest["platform"]), ("sha256:s", "linux-aarch64"))
        self.assertEqual(sorted(p.name for p in final.parent.iterdir()), ["linux-aarch64"], "no partial or old left")
        self.assertEqual((final / bundle.ARCHIVE).stat().st_mode & 0o777, 0o644)
        save = [c for c, _ in self.commands if c[:2] == ["docker", "save"]]
        self.assertEqual(save[0][-1], "sha256:s")

    def test_seal_refuses_a_runtime_layer_missing_a_library(self):
        dest = self.root / "cache"
        for step in ("daemon", "worker", "helper"):
            bundle.build(self.record, step, "debug", 120)
        with self.assertRaisesRegex(Refused, "linux_bundle.py runtime"):
            bundle.seal(self.record, "debug", None, dest, 120)
        self.fetched()
        self.copied = ("libc.so.6", "ld-linux-aarch64.so.1")
        with self.assertRaisesRegex(Refused, "libssl.so.3"):
            bundle.seal(self.record, "debug", None, dest, 120)
        self.copied = LIBRARIES
        self.ldd = LDD + "\tlibmissing.so.1 => not found\n"
        with self.assertRaisesRegex(Refused, "libmissing"):
            bundle.seal(self.record, "debug", None, dest, 120)
        self.assertEqual(list((dest / "9.8.7").iterdir()), [], "nothing placed")

    def test_the_interpreter_tells_a_static_executable(self):
        static, dynamic, text = self.root / "static", self.root / "dynamic", self.root / "text"
        static.write_bytes(elf())
        dynamic.write_bytes(elf("/lib/ld-linux-aarch64.so.1"))
        text.write_text("#!/bin/sh\n")
        self.assertIs(bundle.interpreter(static), False)
        self.assertEqual(bundle.interpreter(dynamic), "/lib/ld-linux-aarch64.so.1")
        with self.assertRaises(Refused):
            bundle.interpreter(text)

    def test_a_failed_seal_leaves_no_partial_folder(self):
        dest = self.root / "cache"
        for step in ("daemon", "worker", "helper"):
            bundle.build(self.record, step, "debug", 120)

        def failing(command, cwd, env=None, timeout=None, log=None):
            if command[:2] == ["docker", "save"]:
                raise Refused("Command exited 1")
            self.execute(command, cwd, env, timeout, log)

        with patch.object(bundle, "execute", failing), self.assertRaises(Refused):
            bundle.seal(self.record, "debug", None, dest, 120)
        self.assertEqual(list((dest / "9.8.7").iterdir()), [])

    def test_image_archive_is_identical_across_seals(self):
        saved = self.root / "saved.tar"
        saved_archive(saved)
        first, second = self.root / "first.tar", self.root / "second.tar"
        bundle.docker_archive(saved, first, self.root)
        # Temporary layer timestamps must not leak into the archive identity.
        import os

        real_open = Path.open

        def opened(path, *args, **kwargs):
            stream = real_open(path, *args, **kwargs)
            if path.name.startswith(".layer-") and args and args[0] == "rb":
                os.utime(path, (2000000000, 2000000000))
            return stream

        with patch.object(Path, "open", opened):
            bundle.docker_archive(saved, second, self.root)
        self.assertEqual(first.read_bytes(), second.read_bytes())

    def test_a_step_s_time_limit_is_bounded(self):
        with patch.object(bundle, "load", lambda path: self.record):
            self.assertEqual(bundle.main(["daemon", "--resources", "r", "--timeout", "301"]), 2)
            self.assertEqual(bundle.main(["daemon", "--resources", "r", "--timeout", "0"]), 2)
        self.assertEqual(self.commands, [])


if __name__ == "__main__":
    unittest.main()

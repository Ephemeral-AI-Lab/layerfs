from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

from check_product_boundary import (RETIRED_PACKAGES, dependency_violations, production_files,
                                    unsafe_violations, violations)


class ProductBoundaryTests(unittest.TestCase):
    def test_native_fuse_owns_requests_without_reverse_daemon_dependency(self):
        path = Path("core/crates/layerfs-fuse/src/ports.rs")
        for source in ["use layerfs_workspace::SourceView;", "use layerfs_overlay::NativeMount;",
                       "use layerfs_content::filesystem::PathName;"]:
            self.assertFalse(violations(path, source))
        for source in ["use layerfs_daemon::Pending;", "use layerfs_persistence::Handles;",
                       "use layerfs_sdk::WorkspaceApi;"]:
            self.assertTrue(violations(path, source))
        self.assertTrue(unsafe_violations(path, "unsafe fn syscall() {}"))
        root = Path("core/crates/layerfs-fuse/src/lib.rs")
        self.assertFalse(unsafe_violations(root, "#![forbid(unsafe_code)]"))
        self.assertTrue(unsafe_violations(root, "mod mount;"))
        prefix = '[package]\nname="layerfs-fuse"\n[dependencies]\n'
        self.assertTrue(dependency_violations(prefix + 'layerfs-daemon={path="../layerfs-daemon"}'))
        self.assertFalse(dependency_violations('[package]\nname="layerfs-daemon"\n[dependencies]\nlayerfs-fuse={path="../layerfs-fuse"}'))

    def test_overlay_terminology_distinguishes_records_from_temporary_buffers(self):
        path = Path("core/crates/layerfs-overlay/src/contract/types.rs")
        for source in ["pub struct Dentry {}", "pub struct ScratchRecord {}",
                       "pub struct IndexedScope {}", "fn f() { StatementKind::Scratch; }"]:
            self.assertTrue(violations(path, source))
        self.assertFalse(violations(path, "pub struct DirectoryEntry {}\npub struct OperationRecord {}"))
        self.assertFalse(violations(path, "fn encode() { let scratch = Vec::new(); } // temporary buffer"))
        sql = Path("core/crates/layerfs-overlay/sql/schema.sql")
        self.assertTrue(violations(sql, "CREATE TABLE scratch (value BLOB);"))
        self.assertFalse(violations(sql, "CREATE TABLE operation_record (value BLOB);"))
        bridge = Path("core/crates/layerfs-bridge/src/native/channel.rs")
        self.assertFalse(violations(bridge, "pub struct ScratchBuffer;"))

    def test_daemon_store_is_provider_independent(self):
        path = Path("core/crates/layerfs-daemon/src/store/open.rs")
        for source in ["use layerfs_storage::Storage;", "use layerfs_history::HistoryCatalog;"]:
            self.assertFalse(violations(path, source))
        for source in ["use layerfs_persistence::Handles;", "use layerfs_sdk::RemoteObjects;",
                       "use rusqlite::Connection;", "use std::path::{Path, PathBuf};"]:
            self.assertTrue(violations(path, source))
        bootstrap = Path("core/crates/layerfs-daemon/src/bootstrap.rs")
        self.assertFalse(violations(bootstrap, "use layerfs_persistence::Handles;"))

    def test_persistence_seal_ffi_is_limited_to_the_authorized_module(self):
        base = Path("core/crates/layerfs-persistence/src")
        ffi = "fn seal() { unsafe { file_control(); } }"
        self.assertFalse(unsafe_violations(base / "backend/sqlite/file_control.rs", ffi))
        for sibling in ["backend/sqlite/seal.rs", "store/seal.rs", "backend/sqlite/connection.rs"]:
            self.assertTrue(unsafe_violations(base / sibling, ffi))
        self.assertFalse(unsafe_violations(base / "lib.rs", "#![deny(unsafe_code)]"))
        self.assertTrue(unsafe_violations(base / "lib.rs", "pub mod store;"))

    def test_retired_transport_paths_cannot_return(self):
        for relative in ("layerfs-api/sdk/src/client/call.rs", "layerfs-api/sdk/src/runtime/owner.rs",
                         "layerfs-daemon/src/upstream/owner.rs", "layerfs-bridge/src/codec/frame.rs",
                         "layerfs-bridge/src/contract/frame.rs", "layerfs-bridge/src/native/framing.rs",
                         "layerfs-api/core/src/lib.rs"):
            findings = violations(Path("core/crates") / relative, "pub use std::io::Read;")
            self.assertTrue(any("retired host-mediated" in message for _, message in findings), relative)

    def test_retired_packages_cannot_return(self):
        self.assertEqual(RETIRED_PACKAGES, (
            "layerfs-server", "layerfs-fuse-legacy", "layerfs-sandbox-legacy",
            "layerfs-sdk-legacy", "layerfs-daemon-legacy", "layerfs-bridge-legacy",
            "layerfs-workspace-legacy",
        ))
        for name in RETIRED_PACKAGES:
            findings = violations(Path("core/crates") / name / "src/lib.rs", "pub use std::io::Read;")
            self.assertTrue(any("retired package" in message for _, message in findings), name)
            self.assertTrue(dependency_violations(f'[package]\nname="{name}"\n'), name)

    def test_sdk_source_edges_and_unsafe_boundary(self):
        path = Path("core/crates/layerfs-api/sdk/src/init.rs")
        self.assertFalse(violations(path, "use layerfs_persistence::Handles;"))
        self.assertTrue(violations(path, "use layerfs_server::Server;"))
        self.assertTrue(unsafe_violations(path, "unsafe fn extend_lifetime() {}"))
        root = Path("core/crates/layerfs-api/sdk/src/lib.rs")
        self.assertTrue(unsafe_violations(root, "mod init;"))
        self.assertFalse(unsafe_violations(root, "#![forbid(unsafe_code)]\nmod init;"))

    def test_sandbox_active_edges_and_unsafe_boundary(self):
        path=Path("core/crates/layerfs-sandbox/src/backend/docker/http.rs")
        self.assertTrue(unsafe_violations(path,"unsafe fn signal_pid() {}"))
        self.assertFalse(unsafe_violations(Path("core/crates/layerfs-sandbox/src/lib.rs"),"#![forbid(unsafe_code)]"))
        prefix='[package]\nname="layerfs-sandbox"\n[dependencies]\n'
        for dependency in ("layerfs-api-core","layerfs-server","layerfs-daemon","layerfs-persistence","layerfs-sdk"):
            self.assertTrue(dependency_violations(prefix+f'{dependency}={{path="../{dependency}"}}'))
        self.assertTrue(dependency_violations('[package]\nname="layerfs-sandbox-legacy"\n[dependencies]\nlayerfs-api-core={path="../layerfs-api/core"}'))

    def test_bridge_control_reuses_domain_records_without_engine_edges(self):
        prefix = '[package]\nname="layerfs-bridge"\n[dependencies]\n'
        for dependency in ("layerfs-content", "layerfs-history"):
            self.assertFalse(dependency_violations(prefix + f'{dependency}={{path="../{dependency}"}}'))
        for dependency in ("layerfs-persistence", "layerfs-sdk", "layerfs-daemon", "layerfs-overlay"):
            self.assertTrue(dependency_violations(prefix + f'{dependency}={{path="../{dependency}"}}'))

    def test_sdk_dependency_edges(self):
        prefix = '[package]\nname="layerfs-sdk"\n[dependencies]\n'
        self.assertFalse(dependency_violations(prefix + 'layerfs-persistence={path="../../layerfs-persistence"}'))
        self.assertTrue(dependency_violations(prefix + 'layerfs-server={path="../../layerfs-server"}'))
        self.assertFalse(dependency_violations(prefix + 'layerfs-project={path="../../layerfs-project"}'))
        self.assertTrue(dependency_violations(prefix + 'layerfs-workspace={path="../../layerfs-workspace"}'))

    def test_daemon_direct_store_and_control_edges_without_host_runtime(self):
        prefix = '[package]\nname="layerfs-daemon"\n[dependencies]\n'
        path = Path("core/crates/layerfs-daemon/src/bootstrap.rs")
        for dependency in ("layerfs-bridge", "layerfs-content",
                           "layerfs-storage", "layerfs-history", "layerfs-persistence"):
            with self.subTest(dependency=dependency):
                self.assertFalse(dependency_violations(prefix + f'{dependency}={{path="../{dependency}"}}'))
                self.assertFalse(violations(path, f'use {dependency.replace("-", "_")}::PublicType;'))
        self.assertTrue(dependency_violations(prefix + 'layerfs-server={path="../layerfs-server"}'))
        self.assertTrue(dependency_violations(prefix + 'layerfs-sdk={path="../layerfs-api/sdk"}'))
        self.assertTrue(violations(path, 'use layerfs_sdk::Runtime;'))
        sdk = '[package]\nname="layerfs-sdk"\n[dependencies]\n'
        self.assertTrue(dependency_violations(sdk + 'layerfs-daemon={path="../../layerfs-daemon"}'))
        self.assertTrue(violations(Path("core/crates/layerfs-api/sdk/src/init.rs"),
                                   'use layerfs_daemon::Owner;'))

    def test_workspace_composition_edges_preserve_domain_and_reverse_boundaries(self):
        for package in ("layerfs-sdk", "layerfs-daemon"):
            source=f'[package]\nname="{package}"\n[dependencies]\nlayerfs-workspace={{path="../layerfs-workspace"}}\n'
            self.assertEqual(bool(dependency_violations(source)), package == "layerfs-sdk")
            path=Path("core/crates/layerfs-api/sdk/src/init.rs") if package=="layerfs-sdk" else Path("core/crates/layerfs-daemon/src/commands.rs")
            self.assertEqual(bool(violations(path,"use layerfs_workspace::Workspace;")), package == "layerfs-sdk")
            self.assertTrue(violations(path,"use layerfs_server::Server;"))
        self.assertTrue(dependency_violations('[package]\nname="layerfs-workspace"\n[dependencies]\nlayerfs-daemon={path="../layerfs-daemon"}\n'))
        self.assertTrue(dependency_violations('[package]\nname="layerfs-content"\n[dependencies]\nlayerfs-workspace={path="../layerfs-workspace"}\n'))

    def test_cluster1_dependency_edges(self):
        for package in ("layerfs-storage", "layerfs-history", "layerfs-project", "layerfs-content"):
            for dependency in ("layerfs-s3", "layerfs-metadata", "layerfs-workspace", "layerfs-sdk"):
                for table in ("dependencies", "build-dependencies", "target.'cfg(unix)'.dependencies"):
                    source = f'[package]\nname="{package}"\n[{table}]\nrenamed={{package="{dependency}",path="../{dependency}"}}\n'
                    self.assertTrue(dependency_violations(source))
                source = f'[package]\nname="{package}"\n[dev-dependencies]\n"{dependency}"={{path="../{dependency}"}}\n'
                self.assertFalse(dependency_violations(source))
        self.assertFalse(dependency_violations('[package]\nname="layerfs-persistence"\n[dependencies]\nlayerfs-history={path="../layerfs-history"}\n'))
        self.assertTrue(dependency_violations('[package]\nname="layerfs-persistence"\n[dependencies]\nlayerfs-workspace={path="../layerfs-workspace"}\n'))

    def test_project_sqlite_backing_uses_the_owning_provider_boundary(self):
        project = '[package]\nname="layerfs-project"\n[dependencies]\n'
        self.assertFalse(dependency_violations(
            project + 'layerfs-storage={path="../layerfs-storage"}\n'))
        provider = '[package]\nname="layerfs-persistence"\n[dependencies]\n'
        self.assertFalse(dependency_violations(
            provider + 'layerfs-storage={path="../layerfs-storage"}\n'
            + 'rusqlite={version="=0.40.2"}\n'))
        for table in ("dependencies", "build-dependencies",
                      "target.'cfg(target_os = \"linux\")'.dependencies"):
            with self.subTest(table=table):
                source = ('[package]\nname="layerfs-project"\n'
                          f'[{table}]\n'
                          'database={package="rusqlite",version="=0.40.2"}\n')
                findings = dependency_violations(source)
                self.assertEqual(len(findings), 1)
                self.assertIn("engine dependency in domain layerfs-project -> rusqlite",
                              findings[0][1])
                self.assertIn("SQLite-backed Project acquisition is allowed",
                              findings[0][1])
                self.assertIn("layerfs-persistence", findings[0][1])

    def test_domain_source_component_names(self):
        for folder in ("layerfs-storage", "layerfs-history", "layerfs-project", "layerfs-content"):
            path = Path("core/crates") / folder / "src" / "implementation.rs"
            self.assertTrue(violations(path, "use layerfs_s3::S3Objects;"))
            self.assertTrue(violations(path, "// layerfs-metadata owns this"))
            self.assertTrue(violations(path, "use layerfs_workspace::Workspace;"))
        path = Path("core/crates/layerfs-persistence/src/implementation.rs")
        self.assertFalse(violations(path, "use layerfs_storage::port::MetadataStore;"))
        self.assertTrue(violations(path, "use layerfs_s3::S3Objects;"))

    def test_storage_encoding_engine_boundary(self):
        for folder in ("encoding", "pack"):
            path = Path("core/crates/layerfs-storage/src") / folder / "read.rs"
            for source in ("use rusqlite::Connection;", "crate::sqlite::lookup::location();"):
                self.assertTrue(violations(path, source))
            self.assertFalse(violations(path, "use crate::source::Source;"))
        self.assertFalse(violations(Path("core/crates/layerfs-storage/src/sqlite/source.rs"), "use rusqlite::Connection;"))

    def test_product_attributes(self):
        for source in (
            "#[test]\nfn case() {}",
            "#[tokio::test]\nasync fn case() {}",
            "#[cfg(any(unix,\n test))]\nmod checks;",
            '#[cfg(feature = "test-instrumentation")]\nfn inject() {}',
            '#[cfg_attr(feature = "checks", test)]\nfn case() {}',
            'fn f() { if cfg!(any(all(unix), test)) {} }',
            'fn f() { if cfg!(any(\n all(unix),\n test\n)) {} }',
        ):
            with self.subTest(source=source):
                self.assertTrue(violations(Path("scope.rs"), source))
        self.assertFalse(violations(Path("scope.rs"), '#[cfg(unix)]\nfn f() { assert!(true); }'))

    def test_entry_files(self):
        for name in ("lib.rs", "mod.rs"):
            path = Path(name)
            self.assertFalse(violations(path, "// documentation\n" * 200))
            self.assertTrue(violations(path, "// documentation\n" * 201))
            self.assertFalse(violations(path, 'mod scope;\npub use scope::Timing;\npub fn run() { scope::run() }'))
            for source in ('pub struct State;', 'impl State {}', 'fn f() { let value = 1; }', 'fn f() { if true {} }'):
                self.assertTrue(violations(path, source))
        self.assertFalse(violations(Path("recording.rs"), "pub struct State;\nimpl State {}"))

    def test_executable_rustdoc(self):
        self.assertTrue(violations(Path("report.rs"), '/// ```rust\n/// let x = 1;\n/// ```'))

    def test_production_file_size_boundary(self):
        for name in ("save.rs", "schema.sql"):
            with self.subTest(name=name):
                source = "\n" * 998 + "final line without newline"
                self.assertFalse(violations(Path(name), source))
                self.assertEqual(violations(Path(name), source + "\nextra")[0][0], 1000)

    def test_unsafe_boundary(self):
        storage = Path("core/crates/layerfs-storage/src")
        audited = storage / "encoding" / "codec.rs"
        elsewhere = storage / "cas" / "store.rs"
        content = Path("core/crates/layerfs-content/src/file/content.rs")
        telemetry = Path("core/crates/layerfs-telemetry/src/timer/recording.rs")
        # The audited module and the lint attribute names never trip the scan.
        self.assertFalse(unsafe_violations(audited, "fn f() { unsafe { call(); } }"))
        self.assertFalse(unsafe_violations(elsewhere, "#![deny(unsafe_code)]\n// the word `unsafe` in a comment\nfn f() {}"))
        self.assertFalse(unsafe_violations(audited, "unsafe fn parse() {}\nfn code() { let _ = unsafe {}; }"))
        # Unsafe code outside the audited module is rejected, comment or not.
        for path in (elsewhere, content, telemetry):
            for source in (
                "fn f() { unsafe { call(); } }",
                "unsafe fn parse() {}",
                "let value = unsafe { std::ptr::read(&x) };",
                "// `unsafe` spelled in a code-like comment does not matter\nfn f() { let _ = 1; }",
            ):
                with self.subTest(path=path, source=source):
                    if "does not matter" in source:
                        self.assertFalse(unsafe_violations(path, source))
                    else:
                        self.assertTrue(unsafe_violations(path, source))
        # The crate roots must declare their documented lint level.
        for path, attr in (
            (storage / "lib.rs", "#![deny(unsafe_code)]"),
            (content.parents[1] / "lib.rs", "#![forbid(unsafe_code)]"),
            (telemetry.parents[1] / "lib.rs", "#![forbid(unsafe_code)]"),
        ):
            with self.subTest(path=path, attr=attr):
                self.assertTrue(unsafe_violations(path, "pub mod run;\n"))
                self.assertFalse(unsafe_violations(path, attr + "\npub mod run;\n"))
        # Paths outside the three known crates are not judged by this rule.
        self.assertFalse(unsafe_violations(Path("crates/other/src/lib.rs"), "fn f() { unsafe {} }"))
        self.assertFalse(unsafe_violations(Path("scope.rs"), "unsafe fn parse() {}"))

    def test_shipped_sql_and_external_tests_scope(self):
        with TemporaryDirectory() as directory:
            core = Path(directory)
            names = ("src/lib.rs", "src/sql/query.sql", "sql/schema.sql", "tests/case.rs",
                     "tests/fixture.sql", "examples/demo.rs", "benches/save.rs")
            for name in names:
                path = core / "crates" / "store" / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("")
            self.assertEqual([str(p.relative_to(core / "crates" / "store"))
                              for p in production_files(core)],
                             ["sql/schema.sql", "src/lib.rs", "src/sql/query.sql"])

    def test_nested_api_product_scope(self):
        with TemporaryDirectory() as directory:
            core = Path(directory)
            for name in ("core/src/lib.rs", "sdk/src/client.rs", "sdk/tests/route.rs",
                         "mcp/src/lib.rs", "cli/src/main.rs"):
                path = core / "crates/layerfs-api" / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("")
            self.assertEqual([str(p.relative_to(core / "crates/layerfs-api"))
                              for p in production_files(core)],
                             ["core/src/lib.rs", "sdk/src/client.rs"])


if __name__ == "__main__":
    unittest.main()

class DurablePersistenceEdges(unittest.TestCase):
    def test_domain_engine_dependencies_are_refused(self):
        for name in ("layerfs-storage","layerfs-history","layerfs-project"):
            for engine in ("rusqlite","postgres","tokio-postgres"):
                self.assertTrue(dependency_violations(f'[package]\nname="{name}"\n[dependencies]\n{engine}="1"\n'))
    def test_persistence_adapter_accepts_its_domain_edges(self):
        self.assertFalse(dependency_violations('[package]\nname="layerfs-persistence"\n[dependencies]\nlayerfs-storage={path="../layerfs-storage"}\nlayerfs-history={path="../layerfs-history"}\n'))

from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

from check_product_boundary import production_files, unsafe_violations, violations


class ProductBoundaryTests(unittest.TestCase):
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

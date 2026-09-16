from pathlib import Path
import unittest

from check_product_boundary import violations


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


if __name__ == "__main__":
    unittest.main()

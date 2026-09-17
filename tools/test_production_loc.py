"""Focused tests for the production LOC counter used by the per-commit rule."""

import tempfile
import unittest
from pathlib import Path

import production_loc


def write(root: Path, relative: str, text: str) -> Path:
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")
    return path


class CounterTests(unittest.TestCase):
    def setUp(self):
        self._temporary = tempfile.TemporaryDirectory()
        self.root = Path(self._temporary.name)

    def tearDown(self):
        self._temporary.cleanup()

    def test_blank_and_comment_lines_do_not_count(self):
        path = write(
            self.root,
            "crates/example/src/lib.rs",
            "//! docs\n\n// a comment\npub fn value() -> u8 {\n    1\n}\n",
        )
        self.assertEqual(production_loc.counted_lines(path), 3)

    def test_nested_block_comments_and_doc_comments_do_not_count(self):
        path = write(
            self.root,
            "crates/example/src/lib.rs",
            "/* outer /* inner */ still */\npub const N: u8 = 1; // trailing\n/// doc\npub const M: u8 = 2;\n",
        )
        self.assertEqual(production_loc.counted_lines(path), 2)

    def test_strings_and_characters_do_not_open_comments(self):
        path = write(
            self.root,
            "crates/example/src/lib.rs",
            'pub const URL: &str = "http://example";\npub const QUOTE: char = \'/\';\nlet x = r#"raw // text"#;\n',
        )
        self.assertEqual(production_loc.counted_lines(path), 3)

    def test_inline_test_modules_are_excluded(self):
        path = write(
            self.root,
            "crates/example/src/lib.rs",
            "pub fn value() -> u8 {\n    1\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn works() {\n        assert_eq!(1, 1);\n    }\n}\n",
        )
        self.assertEqual(production_loc.counted_lines(path), 3)

    def test_cfg_test_items_do_not_hide_following_product_code(self):
        path = write(
            self.root,
            "crates/example/src/lib.rs",
            "#[cfg(test)]\nfn helper() {}\n\npub fn value() -> u8 {\n    1\n}\n",
        )
        self.assertEqual(production_loc.counted_lines(path), 3)

    def test_a_predicate_that_merely_mentions_test_is_production_code(self):
        """`cfg(not(test))`, `cfg(any(test, ...))` and a debug-assertion cfg ship."""
        path = write(
            self.root,
            "crates/example/src/lib.rs",
            "#[cfg(not(test))]\npub fn production_only() -> u8 {\n    1\n}\n\n"
            '#[cfg(any(test, feature = "test-instrumentation"))]\n'
            "pub fn instrumented() -> u8 {\n    2\n}\n\n"
            "#[cfg(any(debug_assertions, feature = \"test-instrumentation\"))]\n"
            "pub fn counted() -> u8 {\n    3\n}\n",
        )
        self.assertEqual(production_loc.counted_lines(path), 12)

    def test_only_a_predicate_a_test_build_satisfies_is_removed(self):
        """`cfg(test)` and `cfg(all(test, ...))` are tests; `cfg(any(test))` too."""
        path = write(
            self.root,
            "crates/example/src/lib.rs",
            "#[cfg(test)]\nfn a() {}\n\n"
            "#[cfg(all(test, unix))]\nfn b() {}\n\n"
            "#[cfg(any(test))]\nfn c() {}\n\n"
            "pub fn value() -> u8 {\n    1\n}\n",
        )
        self.assertEqual(production_loc.counted_lines(path), 3)

    def test_files_reached_only_by_a_test_module_declaration_are_excluded(self):
        """A `#[cfg(test)] mod x;` makes x.rs a test module, not product code."""
        write(
            self.root,
            "crates/example/src/lib.rs",
            "#[cfg(test)]\nmod helpers_tests;\n\npub fn value() -> u8 {\n    1\n}\n",
        )
        write(
            self.root,
            "crates/example/src/helpers_tests.rs",
            "pub fn helper() {\n    assert_eq!(1, 1);\n}\n",
        )
        write(
            self.root,
            "crates/example/src/product.rs",
            "pub fn kept() -> u8 {\n    2\n}\n",
        )
        files = production_loc.scope_files(self.root, "reference")
        names = {path.name for path in files}
        self.assertIn("product.rs", names)
        self.assertNotIn("helpers_tests.rs", names)
        # The two product files count three lines each; the test module counts none.
        self.assertEqual(production_loc.scan(self.root)["scopes"]["reference"]["lines"], 6)

    def test_a_test_only_file_makes_the_modules_it_declares_test_only_too(self):
        """The exclusion is transitive, which is how a nested diagnostic is reached."""
        write(
            self.root,
            "crates/example/src/lib.rs",
            "#[cfg(test)]\nmod native_tests;\n\npub fn value() -> u8 {\n    1\n}\n",
        )
        write(
            self.root,
            "crates/example/src/native_tests.rs",
            '#[path = "issue100_diagnostic.rs"]\nmod issue100_diagnostic;\n',
        )
        write(
            self.root,
            "crates/example/src/issue100_diagnostic.rs",
            "pub fn diagnostic() -> u8 {\n    9\n}\n",
        )
        names = {path.name for path in production_loc.scope_files(self.root, "reference")}
        self.assertNotIn("native_tests.rs", names)
        self.assertNotIn("issue100_diagnostic.rs", names)
        # Only lib.rs is product code here.
        self.assertEqual(production_loc.scan(self.root)["scopes"]["reference"]["lines"], 3)

    def test_sql_comments_do_not_count_but_sql_code_does(self):
        path = write(
            self.root,
            "crates/example/sql/schema.sql",
            "-- a comment\n\nCREATE TABLE t (\n    id INTEGER PRIMARY KEY\n) STRICT;\n",
        )
        self.assertEqual(production_loc.counted_lines(path), 3)

    def test_candidate_runtime_sql_is_counted_for_the_core_scope(self):
        sql = write(
            self.root,
            "core/crates/example/sql/schema.sql",
            "CREATE TABLE t (id INTEGER PRIMARY KEY) STRICT;\n",
        )
        files = production_loc.scope_files(self.root, "core")
        self.assertIn(sql, files)
        self.assertEqual(production_loc.counted_lines(sql), 1)

    def test_reference_runtime_sql_is_still_counted(self):
        sql = write(self.root, "crates/example/sql/schema.sql", "SELECT 1;\n")
        self.assertIn(sql, production_loc.scope_files(self.root, "reference"))

    def test_tests_examples_and_benches_are_excluded(self):
        write(self.root, "crates/example/src/lib.rs", "pub const N: u8 = 1;\n")
        write(self.root, "crates/example/tests/it.rs", "pub const T: u8 = 1;\n")
        write(self.root, "crates/example/examples/demo.rs", "pub const E: u8 = 1;\n")
        write(self.root, "crates/example/benches/bench.rs", "pub const B: u8 = 1;\n")
        relative = [
            str(path.relative_to(self.root))
            for path in production_loc.scope_files(self.root, "reference")
        ]
        self.assertEqual(relative, ["crates/example/src/lib.rs"])

    def test_target_directories_are_excluded(self):
        write(self.root, "crates/example/src/lib.rs", "pub const N: u8 = 1;\n")
        write(self.root, "crates/example/target/generated.rs", "pub const G: u8 = 1;\n")
        relative = [
            str(path.relative_to(self.root))
            for path in production_loc.scope_files(self.root, "reference")
        ]
        self.assertEqual(relative, ["crates/example/src/lib.rs"])

    def test_scan_separates_scopes_and_reports_a_combined_total(self):
        write(self.root, "crates/reference/src/lib.rs", "pub const R: u8 = 1;\n")
        write(self.root, "core/crates/candidate/src/lib.rs", "pub const C: u8 = 1;\n")
        write(self.root, "core/crates/candidate/sql/schema.sql", "CREATE TABLE t (id INTEGER);\n")
        result = production_loc.scan(self.root)
        self.assertEqual(result["scopes"]["reference"]["lines"], 1)
        self.assertEqual(result["scopes"]["core"]["lines"], 2)
        self.assertEqual(result["combined"], 3)

    def test_per_file_reporting_separates_production_and_physical_lines(self):
        write(
            self.root,
            "core/crates/candidate/src/lib.rs",
            "//! docs\n\n// comment\npub const C: u8 = 1;\n",
        )
        report = production_loc.per_file(self.root)
        self.assertEqual(len(report["files"]), 1)
        entry = report["files"][0]
        self.assertEqual(entry["production_loc"], 1)
        self.assertEqual(entry["physical_lines"], 4)
        self.assertEqual(entry["scope"], "core")
        self.assertEqual(entry["crate"], "candidate")

    def test_per_file_and_total_use_the_same_classification(self):
        write(self.root, "crates/reference/src/lib.rs", "// only a comment\n\npub const R: u8 = 2;\n")
        write(self.root, "core/crates/candidate/src/lib.rs", "pub const C: u8 = 3;\n")
        write(self.root, "core/crates/candidate/tests/it.rs", "pub const T: u8 = 4;\n")
        report = production_loc.per_file(self.root)
        by_scope = {"core": 0, "reference": 0}
        for entry in report["files"]:
            by_scope[entry["scope"]] += entry["production_loc"]
        totals = production_loc.scan(self.root)["scopes"]
        self.assertEqual(by_scope["core"], totals["core"]["lines"])
        self.assertEqual(by_scope["reference"], totals["reference"]["lines"])


if __name__ == "__main__":
    unittest.main()

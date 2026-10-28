"""Focused tests for the plugin listing checker (docs/plugins/script/plugin_check.py)."""

import json
import shutil
import tempfile
import unittest
from pathlib import Path

import plugin_check

TEMPLATE = plugin_check.ROOT / "docs" / "plugins" / "template"
SHA = "a" * 40
LAYERFS_SHA = "b5814bf3a8d886ef5e32298118dd1661c352bd2a"
LOGO = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><circle cx="16" cy="16" r="12"/></svg>\n'
README = "Branch Lens draws every agent Branch as a timeline in your editor, live.\n\n## What it adds\n\n- A timeline panel.\n"
INSTALL = ("Install Branch Lens {{version}} for me.\n\n1. Check out LayerFS at {{layerfs.commit}} ({{layerfs.version}}).\n"
           "2. Clone {{repository}} and check out {{commit}}.\n3. Run npm install && npm run build.\n")


def listing(**changes):
    l = {
        "schema": 1, "id": "branch-lens", "name": "Branch Lens",
        "summary": "Every agent Branch as a timeline in your editor.",
        "authors": [{"name": "Ada", "github": "ada"}], "license": "MIT",
        "repository": "https://github.com/ada/branch-lens", "tags": ["editor", "history"],
        "platforms": ["linux"], "uses": ["agent"], "status": "preview",
        "versions": [{"version": "0.1.0", "commit": SHA, "date": "2026-10-23",
                      "layerfs": {"version": "v1.0.0", "commit": LAYERFS_SHA}}],
    }
    l.update(changes)
    return l


class CheckTests(unittest.TestCase):
    def setUp(self):
        self._temporary = tempfile.TemporaryDirectory()
        self.folder = Path(self._temporary.name) / "branch-lens"
        self.folder.mkdir()
        self.write(listing(), README, INSTALL, LOGO)

    def tearDown(self):
        self._temporary.cleanup()

    def write(self, meta=None, readme=None, install=None, logo=None):
        for name, text in (("README.md", readme), ("install.md", install), ("logo.svg", logo)):
            if text is not None:
                (self.folder / name).write_text(text, encoding="utf-8")
        if meta is not None:
            (self.folder / "plugin.json").write_text(json.dumps(meta), encoding="utf-8")

    def errors(self):
        return plugin_check.check_listing(self.folder)

    def assertError(self, fragment):
        self.assertTrue(any(fragment in e for e in self.errors()), f"no error with {fragment!r} in {self.errors()}")

    def test_a_complete_listing_passes(self):
        self.assertEqual(self.errors(), [])

    def test_the_blank_template_fails_on_its_blanks(self):
        shutil.copytree(TEMPLATE, Path(self._temporary.name) / "blank")
        errors = plugin_check.check_listing(Path(self._temporary.name) / "blank")
        for part in ("plugin.json: template blank", "Display page: template blank", "Install prompt: template blank",
                     "Logo: this is the template's placeholder logo", "commit must be the full"):
            self.assertTrue(any(part in e for e in errors), f"{part!r} not in {errors}")

    def test_every_file_is_required(self):
        (self.folder / "install.md").unlink()
        (self.folder / "logo.svg").unlink()
        self.assertError("install.md is missing")
        self.assertError("logo.svg or logo.png is missing")

    def test_versions_are_newest_first_and_pinned(self):
        older = {"version": "0.2.0", "commit": "main", "date": "2026-10-24", "layerfs": {"version": "v1.0.0", "commit": LAYERFS_SHA}}
        self.write(listing(versions=listing()["versions"] + [older]))
        self.assertError("newest first")
        self.assertError("versions[1]: commit must be the full")

    def test_tags_and_fields_are_regulated(self):
        self.write(listing(tags=["Editor", "a", "b", "c", "d", "e", "f"], extra=True))
        self.assertError("1 to 6 tags")
        self.assertError("unknown field extra")
        self.write(listing(tags=["Editor"]))
        self.assertError("lowercase")

    def test_id_must_match_the_folder(self):
        self.write(listing(id="other"))
        self.assertError("it must be the folder's name")

    def test_install_prompt_must_pin_layerfs_and_use_known_placeholders(self):
        self.write(install=INSTALL.replace("{{layerfs.commit}}", "main").replace("{{commit}}", "{{sha}}"))
        self.assertError("must name {{layerfs.commit}}")
        self.assertError("unknown placeholder {{sha}}")

    def test_display_page_rules(self):
        self.write(readme="# Branch Lens\n\n" + README + "\n![shot](shot.png)\n")
        self.assertError("starts at ## headings")
        self.assertError("image shot.png must be an https:// link")

    def test_logo_must_be_square_and_small(self):
        self.write(logo='<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 20"/>' + " " * (65 * 1024))
        self.assertError("must be square")
        self.assertError("keep it under 64 KB")

    def test_main_reports_failures_in_its_exit_status(self):
        self.assertEqual(plugin_check.main([str(self.folder)]), 0)
        (self.folder / "README.md").unlink()
        self.assertEqual(plugin_check.main([str(self.folder)]), 1)


if __name__ == "__main__":
    unittest.main()

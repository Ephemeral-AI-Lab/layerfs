"""External owning checks for input seals, metadata fidelity and failed custody."""
import json
import os
from pathlib import Path
import shutil
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock
import sys

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from r7 import deployment


class DeploymentContract(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory(prefix="layerfs-r7-deployment-test-",dir="/tmp")
        self.base = Path(self.folder.name).resolve()

    def tearDown(self):
        self.folder.cleanup()

    def source(self, name="source", aliases=False):
        root = self.base/name
        root.mkdir()
        (root/"regular").write_bytes(b"complete\x00payload")
        (root/"opaque").symlink_to("/outside/opaque")
        if aliases:
            os.link(root/"regular",root/"alias")
        return root

    def seal(self, root, name="inventory.jsonl", **kwargs):
        return deployment.seal_tree(root,self.base/name,**kwargs)

    def verify(self, root, inventory, reconcile=False):
        # The production CLI guard is separately tested. Only this test's
        # temporary directories are touched; no container or /native access.
        with mock.patch.object(deployment,"target_root",return_value=str(root)):
            return deployment.verify_tree(root,inventory,reconcile=reconcile)

    def plan(self):
        code = self.base/"code"
        code.mkdir()
        (code/"r7_deployment.py").write_bytes(Path(deployment.__file__).read_bytes())
        for helper in ("residency.py","stream_manifest.py","generate_manifest.py","workload.py","oracle.py","changes.py"):
            (code/helper).write_text("sealed static helper\n")
        sealed = self.seal(code)
        tree = dict(source=str(code),target="/code",inventory=sealed["inventory"],inventory_sha256=sealed["sha256"],
                    content_metadata_set_sha256=sealed["content_metadata_set_sha256"])
        plan = dict(schema=deployment.PLAN_SCHEMA,image_id="sealed image",uid=501,gid=20,assets=[tree],native=[],passthrough_mounts=[])
        path = self.base/"deployment.json"
        deployment.write_new(path,plan)
        config = dict(container_setup=dict(manifest=str(path),manifest_sha256=deployment.digest(path),
                      implementation_sha256=deployment.digest(deployment.__file__),wall_stop_seconds=300),
                      identities=dict(image_id="sealed image"),uid=501,gid=20,native_root="/native",socket="/unused/socket")
        return config,dict(case_id="E02",arm="L")

    def test_payload_read_failure_keeps_first_error_after_descriptor_close(self):
        path = self.base / "payload"
        path.write_bytes(b"content")
        original = OSError("first payload read")
        close = deployment.os.close
        def failed_close(descriptor):
            close(descriptor)
            raise OSError("independent descriptor close")
        with mock.patch.object(deployment.os, "read", side_effect=original):
            with mock.patch.object(deployment.os, "close", side_effect=failed_close):
                with self.assertRaises(OSError) as raised:
                    deployment.file_hash(path)
        self.assertIs(raised.exception, original)
        self.assertEqual(original.independent_close_failures, ["independent descriptor close"])

    def test_owned_input_rejects_leaf_and_parent_symlinks_before_resolve(self):
        root = self.base / "actual"
        root.mkdir()
        item = root / "input"
        item.write_bytes(b"sealed")
        alias = self.base / "alias"
        alias.symlink_to(root, target_is_directory=True)
        leaf = self.base / "leaf"
        leaf.symlink_to(item)
        for path in (leaf, alias / "input"):
            with self.assertRaisesRegex(ValueError, "symlink"):
                deployment.owned_input(path)
        self.assertEqual(deployment.owned_input(item), item)

    def test_outputs_exclusive_and_outside_source(self):
        root = self.source()
        self.seal(root)
        with self.assertRaises(FileExistsError):
            self.seal(root)
        with self.assertRaisesRegex(ValueError,"outside source"):
            deployment.seal_tree(root,root/"evidence")

    def test_protected_inputs_and_targets_refused(self):
        with self.assertRaisesRegex(ValueError,"owned /tmp"):
            deployment.owned_input(Path(deployment.__file__))
        for target,role in (("/workspaces","asset"),("/layerfs-store","native"),("/proc","mount"),
                            ("/native/../layerfs-local","native"),("/sys/fs/fuse/connections","mount")):
            with self.assertRaises(ValueError):
                deployment.target_root(target,role)

    def test_relative_parent_and_absolute_inventory_paths_refused(self):
        root = self.source()
        for path in ("../outside","/layerfs-store/store.sqlite","regular/../other","a//b",""):
            with self.assertRaises(ValueError):
                deployment.within(root,path)

    def test_complete_copy_bytes_and_opaque_symlink_are_verified(self):
        root = self.source()
        seal = self.seal(root)
        copied = self.base/"copied"
        shutil.copytree(root,copied,symlinks=True)
        got = self.verify(copied,seal["inventory"],reconcile=True)
        self.assertEqual(got["status"],"PASS")
        self.assertEqual(got["regular_bytes"],len(b"complete\x00payload"))
        self.assertEqual(got["content_metadata_set_sha256"],seal["content_metadata_set_sha256"])
        self.assertIn("NOT_CLAIMED",got["ctime_and_cross_filesystem_inode_equality"])

    def test_transport_nanoseconds_are_reconciled_then_actually_verified(self):
        root = self.source()
        ns = 1_700_000_000_123456789
        os.utime(root/"regular",ns=(ns,ns))
        seal = self.seal(root)
        copied = self.base/"copied"
        shutil.copytree(root,copied,symlinks=True)
        os.utime(copied/"regular",ns=(ns,1_700_000_000_000000000))
        with self.assertRaisesRegex(ValueError,"supported-metadata"):
            self.verify(copied,seal["inventory"])
        result = self.verify(copied,seal["inventory"],reconcile=True)
        self.assertGreater(result["metadata_actions"],0)
        self.assertEqual((copied/"regular").stat().st_mtime_ns,ns)

    def test_same_size_content_mutation_never_passes_actual_verifier(self):
        root = self.source()
        seal = self.seal(root)
        copied = self.base/"copied"
        shutil.copytree(root,copied,symlinks=True)
        info = (copied/"regular").stat()
        (copied/"regular").write_bytes(b"different\x00bytes!")
        os.utime(copied/"regular",ns=(info.st_atime_ns,info.st_mtime_ns))
        with self.assertRaisesRegex(ValueError,"full-byte"):
            self.verify(copied,seal["inventory"],reconcile=True)

    def test_split_hardlink_class_and_unexpected_path_refused(self):
        root = self.source(aliases=True)
        seal = self.seal(root)
        copied = self.base/"copied"
        shutil.copytree(root,copied,symlinks=True)
        with self.assertRaisesRegex(ValueError,"hardlink"):
            self.verify(copied,seal["inventory"],reconcile=True)
        # Independent faithful copy, not an operation retry against a runtime.
        faithful = self.base/"faithful"
        faithful.mkdir()
        shutil.copy2(root/"regular",faithful/"regular")
        os.link(faithful/"regular",faithful/"alias")
        (faithful/"opaque").symlink_to("/outside/opaque")
        shutil.copystat(root,faithful)
        self.assertEqual(self.verify(faithful,seal["inventory"],reconcile=True)["status"],"PASS")
        (faithful/"unexpected").write_bytes(b"x")
        with self.assertRaisesRegex(ValueError,"name inventory"):
            self.verify(faithful,seal["inventory"],reconcile=True)

    def test_retained_copy_projection_uses_no_host_payload_reread(self):
        root = self.source()
        initial = self.seal(root)
        raw = self.base/"closed-fixture.jsonl"
        rows = [dict(schema="r7-fixture-seal-rows-v1",source="/protected/never-opened",copy=str(root))]
        with Path(initial["inventory"]).open() as stream:
            for line in stream:
                row = json.loads(line)
                if "path" in row:
                    rows.append(dict(path=row["path"],copy=row["metadata"],match=True))
        rows.append(dict(event="completed",status="PASS"))
        raw.write_text("".join(json.dumps(row)+"\n" for row in rows))
        with mock.patch.object(deployment,"file_hash",side_effect=AssertionError("host payload reread")):
            projected = self.seal(root,"projected.jsonl",fixture_rows=raw,fixture_rows_sha256=deployment.digest(raw),
                                  expected_set=initial["content_metadata_set_sha256"])
        self.assertEqual(projected["source_payload_bytes_read"],0)
        self.assertEqual(projected["content_metadata_set_sha256"],initial["content_metadata_set_sha256"])

    def test_boolean_cannot_replace_deployment_plan_or_bootstrap_seal(self):
        with self.assertRaises(ValueError):
            deployment.load_plan(dict(helpers_installed=True),dict(case_id="E02",arm="L"))
        config,selected = self.plan()
        self.assertEqual(deployment.load_plan(config,selected)["assets"][0]["target"],"/code")
        config["container_setup"]["implementation_sha256"] = "0"*64
        with self.assertRaisesRegex(ValueError,"implementation"):
            deployment.load_plan(config,selected)

    def test_failed_distinct_action_retains_original_and_never_replays(self):
        config,selected = self.plan()
        container = "a"*64
        output = self.base/"staging"
        with mock.patch.object(deployment.subprocess,"run",return_value=SimpleNamespace(returncode=1)) as invoked:
            with self.assertRaisesRegex(ValueError,"original staging operation failed"):
                deployment.stage(config,selected,container,output)
        self.assertEqual(invoked.call_count,1)
        receipt = json.loads((output/"receipt.json").read_text())
        self.assertEqual(receipt["status"],"FAILED_SETUP")
        self.assertEqual(receipt["container"],container)
        self.assertEqual(receipt["container_stop"],"NOT_ATTEMPTED")
        self.assertFalse(receipt["automatic_retry"])
        self.assertEqual(receipt["operations"][0]["attempts"],1)
        self.assertTrue((output/"001-prepare.stderr").is_file())

    def test_post_ack_input_validation_failure_also_retains_setup_receipt(self):
        config,selected = self.plan()
        config["container_setup"]["manifest_sha256"] = "0"*64
        output = self.base/"staging"
        with mock.patch.object(deployment.subprocess,"run") as invoked:
            with self.assertRaisesRegex(ValueError,"manifest changed"):
                deployment.stage(config,selected,"a"*64,output)
        self.assertEqual(invoked.call_count,0)
        receipt = json.loads((output/"receipt.json").read_text())
        self.assertEqual(receipt["container"],"a"*64)
        self.assertEqual(receipt["operations"],[])
        self.assertIn("manifest changed",receipt["original_error"])

    def test_stage_keeps_exact_original_action_error_and_typed_custody(self):
        config, selected = self.plan()
        original = OSError("original Docker action failure")
        original.original_phase = "original Docker send"
        output = self.base / "original-failure"
        with mock.patch.object(deployment.subprocess, "run", side_effect=original) as invoked:
            with self.assertRaises(OSError) as caught:
                deployment.stage(config, selected, "a" * 64, output)
        self.assertIs(caught.exception, original)
        self.assertEqual(original.staging_output, str(output))
        self.assertEqual(original.staging_receipt["original_error_type"], "OSError")
        self.assertEqual(original.staging_receipt["original_phase"], "original Docker send")
        invoked.assert_called_once()

    def test_receipt_write_failure_cannot_replace_original_setup_failure(self):
        config, selected = self.plan()
        original = ValueError("original sealed plan failure")
        secondary = OSError("independent receipt write")
        with mock.patch.object(deployment, "load_plan", side_effect=original), \
                mock.patch.object(deployment, "write_new", side_effect=secondary) as written, \
                mock.patch.object(deployment.subprocess, "run") as invoked:
            with self.assertRaises(ValueError) as caught:
                deployment.stage(config, selected, "a" * 64, self.base / "receipt-failure")
        self.assertIs(caught.exception, original)
        self.assertEqual(original.independent_output_failures, [str(secondary)])
        self.assertEqual(original.staging_receipt["status"], "FAILED_SETUP")
        written.assert_called_once()
        invoked.assert_not_called()

    def test_selected_output_write_and_independent_close_preserve_original(self):
        original = OSError("original evidence write")
        stream = mock.Mock()
        stream.write.side_effect = original
        stream.close.side_effect = OSError("independent evidence close")
        with mock.patch.object(Path, "open", return_value=stream):
            with self.assertRaises(OSError) as caught:
                deployment.write_new(self.base / "new-evidence", {"value": 1})
        self.assertIs(caught.exception, original)
        self.assertEqual(original.independent_close_failures, ["independent evidence close"])
        stream.close.assert_called_once_with()

    def test_linux_unrepresentable_symlink_mode_rejected_before_any_transfer(self):
        config,selected = self.plan()
        root = self.source("native-source")
        native = self.seal(root,"native.jsonl")
        inventory = Path(native["inventory"])
        rows = [json.loads(line) for line in inventory.read_text().splitlines()]
        for row in rows:
            if row.get("metadata",{}).get("kind") == "symlink":
                row["metadata"]["mode"] = 0o755
        inventory.write_text("".join(json.dumps(row)+"\n" for row in rows))
        plan_path = Path(config["container_setup"]["manifest"])
        plan = json.loads(plan_path.read_text())
        plan["native"] = [dict(source=str(root),target="/native",inventory=str(inventory),inventory_sha256=deployment.digest(inventory),
                               content_metadata_set_sha256=native["content_metadata_set_sha256"])]
        plan_path.write_text(json.dumps(plan))
        config["container_setup"]["manifest_sha256"] = deployment.digest(plan_path)
        selected["arm"] = "N"
        with mock.patch.object(deployment.subprocess,"run") as invoked:
            with self.assertRaisesRegex(ValueError,"UNSUPPORTED_LINUX_SYMLINK_MODE"):
                deployment.load_plan(config,selected)
        self.assertEqual(invoked.call_count,0)


if __name__ == "__main__":
    unittest.main()

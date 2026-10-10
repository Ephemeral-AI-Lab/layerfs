import copy
import importlib.util
from pathlib import Path
import unittest

TOOLS = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("run_confinement", TOOLS / "run_confinement.py")
confinement = importlib.util.module_from_spec(spec)
spec.loader.exec_module(confinement)

DIRECTORIES = ["/workspaces/a", "/workspaces/b"]


def report():
    def cell(group, action, outcome, protected=True, record=False):
        return dict(group=group, target="t", action=action, outcome=outcome,
                    protected=protected and not record, record=record)
    return dict(
        schema="r8b-confinement-probe-v1", cwd=DIRECTORIES[0], sibling=DIRECTORIES[1],
        identity=dict(uid=501, gid=20, groups=[20], status=dict(
            CapInh="0000000000000000", CapPrm="0000000000000000", CapEff="0000000000000000",
            CapAmb="0000000000000000", CapBnd="00000000a82425fb", NoNewPrivs="1", Seccomp="2")),
        own_descriptors={"0": "/dev/null", "1": "pipe:[1]", "2": "pipe:[2]"},
        namespace=dict(unshare_user_mount="EPERM"),
        cells=[cell("direct", "open_read", "EACCES"), cell("direct", "lstat", "ALLOWED", record=True),
               cell("daemon_process", "process_vm_readv", "EPERM"),
               cell("sibling", "listdir", "ALLOWED", record=True)])


class Judge(unittest.TestCase):
    def failures(self, change):
        value = report()
        change(value)
        return confinement.judge(value, DIRECTORIES, 501, 20)

    def test_denied_matrix_with_recorded_cells_has_no_failure(self):
        self.assertEqual(self.failures(lambda value: None), [])

    def test_an_allowed_protected_cell_fails(self):
        def change(value):
            value["cells"][0]["outcome"] = "ALLOWED"
        self.assertEqual(len(self.failures(change)), 1)

    def test_a_recorded_cell_is_never_judged(self):
        value = report()
        self.assertTrue(any(cell["record"] and cell["outcome"] == "ALLOWED" for cell in value["cells"]))
        self.assertEqual(confinement.judge(value, DIRECTORIES, 501, 20), [])

    def test_memory_read_passing_the_access_decision_fails(self):
        def change(value):
            value["cells"][2]["outcome"] = "EFAULT"
        self.assertEqual(len(self.failures(change)), 1)

    def test_identity_capability_and_privilege_bits_are_judged(self):
        for change in (lambda value: value["identity"].update(uid=0),
                       lambda value: value["identity"]["status"].update(CapEff="0000000000200000"),
                       lambda value: value["identity"]["status"].update(CapAmb="1"),
                       lambda value: value["identity"]["status"].update(NoNewPrivs="0"),
                       lambda value: value["identity"]["status"].pop("CapPrm")):
            self.assertEqual(len(self.failures(change)), 1)

    def test_an_inherited_descriptor_fails(self):
        def change(value):
            value["own_descriptors"]["7"] = "/layerfs-store/global/store.sqlite"
        self.assertEqual(len(self.failures(change)), 1)

    def test_namespace_creation_alone_is_recorded_but_protected_access_inside_fails(self):
        def allowed(value):
            value["namespace"] = {"unshare_user_mount": "ALLOWED",
                                  "inside:/layerfs-local": "EACCES", "inside:umount2": "EINVAL",
                                  "inside:mountinfo_has_workspace": True}
        self.assertEqual(self.failures(allowed), [])

        def readable(value):
            allowed(value)
            value["namespace"]["inside:/layerfs-local"] = "ALLOWED"
        self.assertEqual(len(self.failures(readable)), 1)

    def test_a_probe_outside_the_declared_workspaces_fails(self):
        def change(value):
            value["cwd"] = "/tmp"
        self.assertEqual(len(self.failures(change)), 1)

    def test_judging_does_not_alter_the_report(self):
        value = report()
        before = copy.deepcopy(value)
        confinement.judge(value, DIRECTORIES, 501, 20)
        self.assertEqual(value, before)


if __name__ == "__main__":
    unittest.main()

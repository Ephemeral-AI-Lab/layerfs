import importlib.util
from pathlib import Path
import unittest

TOOLS = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("run_mount_holders", TOOLS / "run_mount_holders.py")
holders = importlib.util.module_from_spec(spec)
spec.loader.exec_module(holders)

SIBLING = "Unmounted, Gone observed"


class Judge(unittest.TestCase):
    def test_reversible_busy_with_the_mount_present_passes(self):
        self.assertEqual(holders.judge(dict(sibling_unmount=SIBLING, unmount_while_held="BUSY",
                                            held_mount_present_after_busy=True)), [])

    def test_busy_without_the_mount_fails(self):
        self.assertEqual(len(holders.judge(dict(sibling_unmount=SIBLING, unmount_while_held="BUSY",
                                                held_mount_present_after_busy=False))), 1)

    def test_complete_unmounted_passes_only_with_absence_and_gone(self):
        complete = dict(sibling_unmount=SIBLING, unmount_while_held="UNMOUNTED",
                        held_mount_present_after_unmount=False, cleanup_after_holder_exit="Gone")
        self.assertEqual(holders.judge(complete), [])
        self.assertEqual(len(holders.judge(dict(complete, held_mount_present_after_unmount=True))), 1)
        self.assertEqual(len(holders.judge(dict(complete, cleanup_after_holder_exit="Held"))), 1)

    def test_retained_custody_and_unknown_outcomes_fail(self):
        for outcome in ("RETAINED", "FAILED", None):
            self.assertEqual(len(holders.judge(dict(sibling_unmount=SIBLING,
                                                    unmount_while_held=outcome))), 1)

    def test_a_held_sibling_fails_independently(self):
        self.assertEqual(len(holders.judge(dict(sibling_unmount="FAILED: Busy", unmount_while_held="BUSY",
                                                held_mount_present_after_busy=True))), 1)

    def test_the_busy_marker_is_the_exact_typed_refusal(self):
        text = ("original runtime failure: original_failure=SDK operation: Remote(ControlRefusal "
                "{ code: Busy, phase: \"unmount:kernel\", moved: None, published: None, detail: "
                "\"kernel reported the mount busy; no terminal effect\" })")
        self.assertTrue(all(part in text for part in holders.BUSY))
        self.assertFalse(all(part in text.replace("unmount:kernel", "unmount:drain")
                             for part in holders.BUSY))


if __name__ == "__main__":
    unittest.main()

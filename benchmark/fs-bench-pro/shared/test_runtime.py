import sys
import unittest
from unittest.mock import patch

import runtime


class RuntimeTests(unittest.TestCase):
    def test_bounded_output_and_timeout(self):
        result = runtime.run([sys.executable, "-c", "print('x'*100000)"], deadline=runtime.Deadline.after(2), output_limit=16)
        self.assertTrue(result.truncated)
        self.assertLessEqual(len(result.stdout) + len(result.stderr), 16)
        result = runtime.run([sys.executable, "-c", "import time;time.sleep(2)"], deadline=runtime.Deadline.after(.05), check=False)
        self.assertTrue(result.timed_out)
        self.assertNotEqual(result.returncode, 0)

    def test_volume_rejected(self):
        value = {"Image": "image", "State": {"Running": True}, "Mounts": [{"Type": "volume"}]}
        with self.assertRaisesRegex(runtime.RuntimeFailure, "runtime mount"):
            runtime._validate_sample_inspection(value, "image", {}, 2, 2048, 256)

    def test_active_cache_entry_is_not_evicted(self):
        cache = runtime.PreparedCache(max_entries=2, max_bytes=100)
        entries = [runtime._CacheEntry("active", "owned:active", "a"*64, "1", 10, {}),
                   runtime._CacheEntry("idle", "owned:idle", "b"*64, "2", 10, {})]
        with patch.object(cache, "entries", return_value=entries), patch.object(cache, "_in_use", side_effect=lambda image, deadline: image == "active"), patch.object(runtime, "run") as command:
            removed = cache.evict(deadline=runtime.Deadline.after(1), incoming_entries=1)
            self.assertEqual(removed, ["idle"])
            self.assertEqual(command.call_args.args[0], ["docker", "image", "rm", "owned:idle"])

    def test_all_protected_cache_fails_without_delete(self):
        cache = runtime.PreparedCache(max_entries=1, max_bytes=100)
        entries = [runtime._CacheEntry("active", "owned:active", "a"*64, "1", 10, {})]
        with patch.object(cache, "entries", return_value=entries), patch.object(runtime, "run") as command:
            with self.assertRaisesRegex(runtime.RuntimeFailure, "protected/active"):
                cache.evict(deadline=runtime.Deadline.after(1), incoming_entries=1, protected=["active"])
            command.assert_not_called()


if __name__ == "__main__":
    unittest.main()

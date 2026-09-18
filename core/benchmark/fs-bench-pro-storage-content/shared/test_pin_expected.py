#!/usr/bin/env python3
"""The pinned-constant generator, through `unittest`."""

import unittest

import pin_expected


class PinExpectedSelfCheck(unittest.TestCase):
    def test_self_check(self):
        self.assertEqual(pin_expected.self_check(), [])

    def test_instrumentation_is_never_pinned(self):
        self.assertIn("timing_json_bytes", pin_expected.INSTRUMENTATION_KEYS)


if __name__ == "__main__":
    unittest.main()

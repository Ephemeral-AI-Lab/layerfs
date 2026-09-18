#!/usr/bin/env python3
"""The four-phase composition and its reconciliation, through `unittest`."""

import unittest

import phases


class PhasesSelfCheck(unittest.TestCase):
    def test_self_check(self):
        self.assertEqual(phases.self_check(), [])

    def test_the_declared_tolerance_is_stated(self):
        self.assertEqual(phases.tolerance_ns(0), phases.TOLERANCE_ABSOLUTE_NS)
        self.assertGreater(phases.tolerance_ns(10_000_000_000), phases.TOLERANCE_ABSOLUTE_NS)


if __name__ == "__main__":
    unittest.main()

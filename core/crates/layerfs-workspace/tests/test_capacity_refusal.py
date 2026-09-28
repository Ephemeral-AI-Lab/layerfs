"""External refusal parser's fail-closed completeness and boundary tests."""
import unittest
from capacity_refusal import parse


class CapacityRefusalParserTest(unittest.TestCase):
    def test_memory(self):
        result = parse('prefix\nLFS_CAPACITY_REFUSAL v=1 domain=memory operation=reserve '
                       'site=commit/active_reconcile.rs:12 used=800 request=300 limit=1000\n')
        self.assertEqual(result['refusals'][0]['request'], 300)
        self.assertEqual(result['refusals'][0]['domain'], 'memory')

    def test_physical(self):
        result = parse('LFS_CAPACITY_REFUSAL v=1 domain=physical operation=metadata_reserve '
                       'site=active/pages.rs:214 allocated=4096 reserved=4096 request=4096 limit=8192')
        self.assertEqual(result['refusals'][0]['reserved'], 4096)

    def test_no_inference_from_estimate_or_absent_line(self):
        self.assertEqual(parse('pressure=true reserve_estimate=67239936')['refusals'], [])

    def test_c5_known_without_local_install(self):
        context = ('LFS_C5_FAILURE_CONTEXT v=1 known=true installed_revision=NA '
                   'class=capacity budget_used=1000 allocated=4096 reserved=0 '
                   'quota=67108864 active_revision=300 pages=8 pins=2')
        record = parse(context)['c5_contexts'][0]
        self.assertTrue(record['known'])
        self.assertEqual(record['installed_revision'], 'NA')
        self.assertEqual(record['pins'], '2')
        self.assertEqual(parse(context)['refusals'], [])

    def test_missing_or_nonrefusal_rejected(self):
        for line in (
            'LFS_CAPACITY_REFUSAL v=1 domain=memory operation=reserve site=a:1 used=2 request=1',
            'LFS_CAPACITY_REFUSAL v=1 domain=physical operation=metadata_reserve site=a:1 allocated=2 reserved=3 request=1 limit=6',
            'LFS_CAPACITY_REFUSAL v=1 domain=physical operation=reserve site=a:1 allocated=2 reserved=3 request=1 limit=5',
        ):
            with self.subTest(line=line), self.assertRaises(ValueError):
                parse(line)


if __name__ == '__main__':
    unittest.main()

# The C5 context alone is not proof of which request failed: both records
# are required for an attribution in a real stderr receipt.

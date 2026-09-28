import unittest
from active_rca import parse_rows, summarize

ROW = ('LFS_INDEX_PAGE_CAUSE v=1 generation=2 revision=11 update_keys=3 '
       'selected_height=2 new_height=2 captured=0 frozen_revisions=0 direct=false '
       'changed_leaf=1 changed_parent=2 direct_leaf=0 direct_parent=0 '
       'admission_no_key=0 normalization_no_key=1 connection_no_key=0 '
       'height_pages=0 index_pages=4 directory_pages=1 created_total=5 '
       'replaced=3 hot_before=1 hot_after=1')
CAUSE = ('GENERIC_WRITE_CAUSE writes=1 hot=0 admissions=0 normalizations=1 '
         'representation_only_pages=1 seeks=15 index_fetches=0 '
         'index_writes=5 pack_writes=1')


class ActiveRcaTest(unittest.TestCase):
    def test_one_prepared_write(self):
        result = summarize(ROW, CAUSE, 1)
        self.assertEqual(result['totals']['normalization_no_key'], 1)
        self.assertEqual(result['first_revision'], 11)

    def test_v2_splits_separate_from_page_roles(self):
        row = ROW.replace('v=1', 'v=2') + (' generic_split_events=1 '
                    'generic_split_pages=2 direct_carries=0')
        parsed = parse_rows(row)[0]
        self.assertEqual(parsed['generic_split_pages'], 2)
        self.assertEqual(summarize(row, CAUSE, 1)['totals']['generic_split_events'], 1)
        with self.assertRaisesRegex(ValueError, 'split/carry'):
            parse_rows(row.replace('generic_split_pages=2', 'generic_split_pages=1'))

    def test_missing_not_zero_and_wrong_total(self):
        with self.assertRaisesRegex(ValueError, 'distinct prepared'):
            summarize('', CAUSE, 1)
        with self.assertRaisesRegex(ValueError, 'invalid page-cause'):
            parse_rows(ROW.replace('index_pages=4', 'index_pages=5'))
        with self.assertRaisesRegex(ValueError, 'role totals'):
            summarize(ROW, CAUSE.replace('representation_only_pages=1',
                                        'representation_only_pages=0'), 1)

    def test_prior_c5_capture_excluded_and_directory_counted(self):
        previous = ROW.replace('revision=11', 'revision=10').replace('captured=0', 'captured=1')
        result = summarize(previous+'\n'+ROW, CAUSE, 1)
        self.assertEqual(result['excluded_prior_c5_revisions'], [10])
        self.assertEqual(result['totals']['index_pages']+result['totals']['directory_pages'], 5)

    def test_duplicate_revision_rejected(self):
        with self.assertRaisesRegex(ValueError, 'distinct prepared'):
            summarize(ROW+'\n'+ROW, CAUSE.replace('writes=1', 'writes=2'), 2)


if __name__ == '__main__':
    unittest.main()

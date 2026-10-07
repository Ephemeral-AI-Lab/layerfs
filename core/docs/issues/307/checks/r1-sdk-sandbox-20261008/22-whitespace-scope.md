# R1a raw-output and source whitespace scope

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

The first full `git diff --cached --check` after staging raw test receipts exited2:
four Rust test stdout files end in the test runner's original blank line at EOF:
06-sdk-tests.txt, 07-control-tests.txt, 14-final-sdk-tests.txt and
15-final-control-tests.txt. Those raw outputs are preserved byte-identically;
they are data, not product/document formatting. No product test/check failed
and no raw output is trimmed/replaced to manufacture a full-scope whitespace PASS.

The required source/document whitespace check excludes exactly those four raw
stdout artifacts with Git literal pathspecs; every source, test source, Markdown,
JSON, build/check receipt and staged path outside those four remains in scope.
Its actual command/output is retained in23-source-whitespace.txt. The original
full-scope diagnostic remains recorded here, not relabelled as passing. No
.gitattributes/guard change, test rerun or profile/cache/workload change follows.
Production LOC computation completed before that whitespace diagnostic and
records165813→166022 (delta+209). Final staged/committed product identities are
confirmed independently; adding documentary evidence changes no counted source.

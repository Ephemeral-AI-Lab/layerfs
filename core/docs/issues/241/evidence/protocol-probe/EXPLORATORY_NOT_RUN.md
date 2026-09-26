# Exploratory v2 STATE/EDIT/ACK selection — NOT_RUN

The already frozen [CONTRACT.md](CONTRACT.md) describes STATE size 64,
EDIT size 4,160, and an optional test-only ACK. **No mounted attempt was made
for any of its three cases** (`state`, `edit_ack`, `post_error_query`). The
owner selected a smaller final product candidate before sampling: one
88-byte read-only STATE and one 4,192-byte EDIT, without product ACK or a
nonce slot. The unused exploratory test source was removed before any run.
Do not cite this selection as Linux capability evidence or silently relabel
the final candidate's receipts as these cases.

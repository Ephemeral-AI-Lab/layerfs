# Family7 cause diagnostic r074 —setup INCOMPLETE, sample NOT_RUN

**Status: Dated diagnostic checkpoint; no product measurement.**

Committed sourcec4b802484 entrypoint imports fail before acquisition:
`Case.__init__() got an unexpected keyword argument 'environment'`.
Existing native invocation already forwards an optional case environment, but
its base dataclass lacked that declared field. No build, lock, prepared clone,
Docker resource or timed child was acquired. The selected diagnostic is NOT_RUN,
with sample_count0. [Compact receipt](20260930-workspace-shell-package-cause-r074-receipts.json)
retains command/source/tree and raw manifest hash; prior SDK failures unchanged.

Fix the harness dataclass declaration with default empty environment, run focused
package import/registry guards, then the same count-driven diagnostic once at
new committed source/fresh r075 output. No SDK resample or product/limits change.
Production135672(reference65417/Core70255) unchanged, delta0; PhaseB+233.

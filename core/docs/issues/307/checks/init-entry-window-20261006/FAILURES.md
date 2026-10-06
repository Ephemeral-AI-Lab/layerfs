# Retained failures and evidence limits

- 02-before-count-regression: partial test name with --exact selected zero bodies;
  not proof. Corrected selection reused the unchanged no-run binary.
- 03-before-count-corrected: intended baseline count regression FAILED once:
  32-file window executed68 statements/5353 VM, above the new bounded execution
  assertion. No timeout or performance sample. Later proof does not relabel it.
- 04-after-build: FAILED compile, no tests ran. Extracted match needed parentheses,
  a borrowed binding vector needed release before clearing input padding, and
  a moved import was unused. Source/output diagnosed and corrected before05.
- Provider tests have post-execution binary hashes only; consumer binaries have
  matching pre/post hashes. No missing earlier pre-hash is invented.
- Global Store runtime is unsupported on Linux; Docker checks are compilation/
  Clippy and do not qualify acquisition provider behavior on Linux.
- Cache is uncontrolled for functional/count checks; no speed/RSS/rate admission.
  Actual acquisition capacity/unknown outcomes are not induced by these tests;
  existing Session quarantine/rollback public proofs retain their owning scope.

# Exact unlink-only source compatibility

Report default: `assets-34224330`, revision `3422433020a678a77f88e8a110492ca293c05e30`, product seal
`4637a27f57351decbee4f800ba97f63d743fb03c7c5b91bad56550eadb310170`. Mapping SHA-256: `35d7b465236986372549210a892cdf0db706b01d27a982796032f43df43ab1c9`.
The preceding map is preserved in `source-bridge-before-34224330.json`.

Retain96 completed performance slots: payload24, directory36, and tiny create/
stat/bulk-create36. Of these,95 use fbf and tiny-stat-1 seed1 uses b8's valid
atomic-sampler replacement. Retain only the old payload-create-1m seed1 proof.
All tiny unlink/bulk-delete slots, all Git slots and the old bulk-delete proof
use new default assets and require corrected-source execution.

Two explicit product bridges bind complete old/new product trees. The only
normalized runtime body is the exact `ProxyClient::unlink` method, and the
only other exclusion is its `#[cfg(test)]` module; both old/new excluded bodies
are individually hashed. All surrounding source, object layouts, product files,
Cargo inputs and normative contracts must match. The former product seal is
`e24867af45d83c455dbfac530d43140fec7cdc40d3eae9ff70a30883d239125a`. Every retained attempt must additionally
prove complete zero `callback_unlink` and `callback_rmdir` observations. Any
additional product delta or missing/nonzero callback observation rejects reuse.

Four verification bridges preserve exact ordinary definitions and generators.
Only the resource sampler body may differ for fbf; signatures and all other
registry bytes stay fixed. The sustained-only helper change is not an ordinary
family dependency. No old producing source, image or product seal is relabeled.

Helper validation passed all16 selectors, four verification bridges and two
exact product proofs. Inventory assertions established precisely96 retained
performance slots, one b8 override, the excluded deleting/Git cases, and the
single retained proof. Per-attempt zero-callback checks remain mandatory in
full report validation; this configuration check does not waive them. No build,
benchmark, preparation or full report regeneration ran.

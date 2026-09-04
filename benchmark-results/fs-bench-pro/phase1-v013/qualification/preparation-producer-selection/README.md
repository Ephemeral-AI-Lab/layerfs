# Separate sealed preparation producer

`workspace-runner.py --assets RUNTIME --preparation-assets PRODUCER` selects a
producer only for immutable Store/fixture/reference acquisition. Omitting the
option preserves the existing single-build behavior. The current runner must
still exactly match the runtime build's committed runner/custody/runtime scripts;
the historical producer instead passes sealed-manifest, binary, source, image,
image-binary and preparation-compatibility validation. No old producer script is
executed and no mutable result is cached.

The producer and runtime must have the same initialization/generator digest,
frozen execution/family specifications and complete registry. Each selected
fixture and Git-reference info command runs on both binaries with the producer's
image identity, and the descriptors/plans must match before acquisition. The
existing custody publisher receives only the producer binary, producer build
evidence, producer image, expected plan and existing compatibility key. Hits
retain their original producer, input/oracle identity, master manifest and key.
A separate `preparation/producer-selection.json` records the explicitly selected
producer; it never overwrites master provenance. Runtime creation, sampling,
workload execution and outcome identities continue to use `--assets`.

Git additionally requires the reviewed Dockerfile SHA-256
`7271d9f0437152402d556d3a0d7804f4a3e0fb4a3fdf5f59d2c1f87ac8166023`, matching Linux
platform/native configuration/PATH, and identical nonempty immutable system
layers. This exact recipe has four generated-file COPY layers (workload,
daemon, FUSE, entrypoint) and one chmod/self-check layer after that prefix;
those are the only five excluded layers. They do not install or replace Git,
its libraries or configuration. A different recipe or system identity fails
closed for review. Source revision/tree/seal environment values alone may differ;
other environment and runtime configuration must match.

Use `assets-34224330` as the explicit stable producer for both the next-candidate
Git performance and matching verification. The existing successful Git10 seed1
attempt `cd922cae2006` already used its Store key
`cf713b8e161e040da62bcf171c1fc55e66113dfa3b8a5aff98432b22f5389b2d` (input plan
`0c7b19a6d95fa3c556b2cd3545038691cab8a0fbfbf14ea129a6dc1eb3ae1375`) and reference key
`889d3bb87f25b37fe2930ce3093540b7169021bea3e44593a96733e708ef6284` (reference plan
`68e42f4c8fc834e22c1f603bb22656ebe9a39c8c3cf48459565dcee3644651e5`). An earlier b8
producer can be selected explicitly for a case whose retained performance used
that producer, subject to the same checks. Do not switch all cases globally to
b8 and thereby change the expected input identity of existing342 evidence.

The one focused `check.py` selection/acquisition model passed. It compares the
actual sealed b8/342 system metadata, verifies all mismatch rejection paths,
separates runtime and producer arguments, checks both Store and reference
acquisition, preserves original master provenance, and checks in-run acquisition
reuse. It mocks binary validation and publisher execution, so it is not a live
cache-hit or product qualification. No benchmark binary, container, build or
performance measurement was run. The exact runner source hash is in result.json.

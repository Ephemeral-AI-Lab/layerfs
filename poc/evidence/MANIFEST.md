# Portable evidence manifest

SHA-256 values below bind the portable files copied from the original ignored
`target/` evidence. The campaign's historical aggregate hash remains
`ba53703603f2513d7864bb6e5ffb2948e38371e1f1f06ac108cad9ef3a36445f`;
that aggregate includes original pathnames, so the portable copy is verified by
the per-file hashes instead.

## Pre-repair A01–A17 campaign

```text
campaign-time.txt  1c5ddaa593b4eff505a3d948adc7a42ba30e72d3790338fcc558963f78d3498b
environment.json   c384fa28310e67691bf691cf205cd7e6c99f63910bd026daf2c6fcb662b09768
master.json        9a1fae41c1d6ce8075777989ff2f9946396475ae486a2512be61c0c80b9e1ca2
rows.jsonl         af422d7f1eea02e995136524414b6150f407bec0bab472396faa2cf66a266b68
schedule.json      5a57aa55957a6077d2b681cf3b31f7506189265a7354d3a16a7adeaad93c22ed
summary.json       0fa7e3ec0279a453730ccfde216f22c5c7b79f1a850b39d57a301a70f807eaa4
summary.md         c1cedaa80b805ea5bf204abd6f0eecbf1a6f0d3fdc855866c84ad601d4b9a7de
```

## A02 diagnostic

```text
preregistration.json  918fcef3aae654003dbeb5f62609b8e296cc34bb6d8cf4f1c2f3793d9b8d9254
result.json           0d7ecc3cfbceb75239137616d130208f3d7e29c776c1388d24f3a204e54a184f
trace.tsv             a24b4670fbaea926dc493909b3dfc50923667d8f2483897ceabd817c7b6ec790
```

`result.json` contains BLAKE3 custody references for all nine child receipts;
the portable `children/` directory retains those exact
receipts and their empty stdout/stderr witnesses.

The post-repair closure manifest is generated only after source settlement and
is stored under `stage1-post-repair-closure/`.

## Post-adversarial-repair closure

```text
closure.json                         fe7c10d008d898d850e4ad702702218d0bc3cc3d5029bb686cd839dfa0beb36f
closure-v2-clippy-failure.typescript cd79d95ee18b0b5994025ce8a4b2134e4a71ac5a65f788180851e3df3dfd3585
closure-v3.typescript                6c8e91582d815fd5d38bb9ca930de00802d5e2a7e1c194f834ac3263accceb9a
closure-v4-superseded.typescript     89ccca84463a3baefbf3e916f28e1fc9eb768f7f2b61dae76e97916445d477b1
closure-v5.typescript                0fbfffe6c695fa6615fae5ff8e7237c3fc085e5bfa64a6f2fa08ddd3b96abbf4
run-closure.zsh                      b247941f68f82947b236ca184463aeb37908ed123c15d3f0ea15d4539ada5305
run-touched-closure-v4.zsh           677f069f74977b816aa2cfc119a3067687a7dadedc1f9a0eebcf69342b52c3d0
run-touched-closure-v5.zsh           677f069f74977b816aa2cfc119a3067687a7dadedc1f9a0eebcf69342b52c3d0
readiness-v4-superseded.json         fb0579e3d107dff97549395a155c6b1fb1e3c03b32164b70992c9e53b848d6be
readiness.json                       ecb99f0343265570f12f8a2e550d83cb54014a89a4e4de68ca331a5e9146bfff
```

## Stage 1.1 Apple/APFS terminal receipt

The compact receipt binds the exact source/executable/fixture and records the
hashes of every immutable attempt-014 raw artifact. The raw 47-row campaign is
retained under `target/layerfs-stage1-apple-edge-20260825-attempt-014`.

```text
stage1.1-apple-edge-20260825/terminal-receipt.md  0fe48f8cd68fc50a003d6e9d34a675efedee178e5997d5cb03a74d250b2f6e17
```

## Stage 1.1M current-source Verified closure

The compact receipts bind the frozen M7 performance operand separately from
the clean current-source release and independently recomputed attempt-015
correctness regression. The raw campaign remains append-only under
`target/layerfs-stage1-apple-edge-20260825-attempt-015`.

```text
stage1.1m-current-source-closure-20260825/closure.json             459ca0af8c63d1f2140248dd872b68b255c0e64b5bd4afb39b702b088e4c261a
stage1.1m-current-source-closure-20260825/regression-receipt.json  e5aceda23ffb86a07f21f927eb004d8f39e00a1cbed3f0559a4cba6fc7154c68
stage1.1m-current-source-closure-20260825/independent-audit.json   2fde59d6be6595a5538a404a4b6663a89400dc45133aa793182db2cfc1871b12
stage1.1m-current-source-closure-20260825/terminal-receipt.json    07f1628689abc8aff0882a880359c6754939c1a562f83d40d525f15369e3bc96
stage1.1m-current-source-closure-20260825/summary.md               e540e5fe0c5ae28b29c81289ac3da5713e8fd52f1245f7c12351778ef8ac3cfc
```

## Stage 1.1T TrustedLocalDev materialization

These compact receipts bind the separate weaker-trust product class and never
replace or relabel the Verified Stage 1.1M result. Raw rows remain under
`target/layerfs-stage1t-trusted-20260826-attempt-001`.

```text
stage1.1t-trusted-20260826/terminal-receipt.json  f5b4e7ce3b02423647abef25fab1801786b6b86ef0313d0738e8e801bccf4177
stage1.1t-trusted-20260826/independent-audit.json 2f20dd2be7a457aa974d2f3b8a8eb0602b36acec50492c244f1c5fcbdb164626
stage1.1t-trusted-20260826/summary.md              ad38fddfb05ccef25e27348cf891fc03b08c428432e2a06b1f6f901819283cee
```

Attempt-001 remains byte-for-byte preserved. Its append-only supersession and
the historical attempt-002 receipts are:

```text
stage1.1t-trusted-20260826/supersession.json                     57b4971d7065f754e4ec5ceb4bf9a6a0166e7a8d23c325d3db36691d95589732
stage1.1t-trusted-20260826-attempt-002/terminal-receipt.json      fae29ddf95ed16f33520ec13a56b575cec00f7f006e24d2df06240915511c181
stage1.1t-trusted-20260826-attempt-002/independent-audit.json     3d79427a1e581306ca658cb4821058674b8fe1881b314167ce4bade50d965857
stage1.1t-trusted-20260826-attempt-002/summary.md                 d50b3888be832e384cacc8f196ee8c2974fec3aea4b2327bf272c57e6999cb71
```

Attempt-002 remains byte-for-byte preserved, but its exact row-timer and build
custody claims are superseded. The independently audited v2 population and
current-source regression receipts are:

```text
stage1.1t-trusted-20260826-attempt-003/custody-receipt.json     7bf7a7e66124c555001cec35c31897448385e5e7174b046672833fd8e3e8a55c
stage1.1t-trusted-20260826-attempt-003/independent-audit.json   9de26d3eaafe9c465f12a67221b1744985fb23584e1acc28b99042f56b774a8a
stage1.1t-trusted-20260826-attempt-003/regression-receipt.json  f8192c65c44010242a99ec2ccb59cca827cb4b2975cdc1329e9ec5bddf973fa2
stage1.1t-trusted-20260826-attempt-003/summary.md                a77dcbd15ddfb8be956850f78f0954da7e64eaf66bffe855a771017707d4b025
stage1.1t-trusted-20260826-attempt-003/supersession.json        dc70bd1747c531c6d8bdb5f99bfdd2c9d09b8f9434d6061b591759360ad70859
stage1.1t-trusted-20260826-attempt-003/terminal-receipt.json     94a9494a039143f69d2e380fb4e01b3bf6c801c1ff7bd4a7590909ba6fc4edbf
```

The attempt-003 files bind raw evidence under
`target/layerfs-stage1t-trusted-20260826-attempt-003`, the clean observed build
at `3635dfc`, and the final current-source Stage 1.1 attempt-020 regression at
`36d05d8`. Trusted is never relabeled Verified; the Verified performance
result remains `REVISE_NO_AUTHORIZED_OWNER`.

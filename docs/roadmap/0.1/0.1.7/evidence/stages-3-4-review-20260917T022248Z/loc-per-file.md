| path | action | before | after | delta | plan range | verdict | physical |
| --- | --- | ---: | ---: | ---: | --- | --- | ---: |
| core/crates/layerfs-content/src/error.rs | Update | 116 | 120 | +4 | 120-190 | within | 174 |
| core/crates/layerfs-content/src/file/cdc/gear.rs | Retain | 494 | 494 | +0 | 494 | within | 538 |
| core/crates/layerfs-content/src/file/cdc/mod.rs | Retain | 5 | 5 | +0 | 5 | within | 10 |
| core/crates/layerfs-content/src/file/content.rs | Update | 195 | 197 | +2 | 150-230 | within | 254 |
| core/crates/layerfs-content/src/file/edit/apply.rs | New | 0 | 389 | +389 | 130-230 | above | 454 |
| core/crates/layerfs-content/src/file/edit/compare.rs | New | 0 | 67 | +67 | 80-140 | below | 86 |
| core/crates/layerfs-content/src/file/edit/concat.rs | New | 0 | 19 | +19 | 210-350 | below | 29 |
| core/crates/layerfs-content/src/file/edit/finish.rs | New | 0 | 38 | +38 | 110-190 | below | 54 |
| core/crates/layerfs-content/src/file/edit/frontier.rs | New | 0 | 0 | +0 | 180-300 | below | 0 |
| core/crates/layerfs-content/src/file/edit/input.rs | New | 0 | 286 | +286 | 90-160 | above | 390 |
| core/crates/layerfs-content/src/file/edit/mod.rs | New | 0 | 15 | +15 | 8-16 | within | 20 |
| core/crates/layerfs-content/src/file/edit/split.rs | New | 0 | 23 | +23 | 160-260 | below | 32 |
| core/crates/layerfs-content/src/file/edit/tree.rs | ADDED (unplanned) | 0 | 701 | +701 | (no plan row) | - | 901 |
| core/crates/layerfs-content/src/file/mapping/build.rs | Update | 236 | 309 | +73 | 220-330 | within | 391 |
| core/crates/layerfs-content/src/file/mapping/codec.rs | Update | 303 | 303 | +0 | 280-380 | within | 334 |
| core/crates/layerfs-content/src/file/mapping/mod.rs | Update | 15 | 15 | +0 | 18-30 | below | 20 |
| core/crates/layerfs-content/src/file/mapping/predecessor.rs | New | 0 | 0 | +0 | 100-180 | below | 0 |
| core/crates/layerfs-content/src/file/mapping/read.rs | Update | 210 | 210 | +0 | 180-290 | within | 241 |
| core/crates/layerfs-content/src/file/mapping/types.rs | Update | 182 | 182 | +0 | 180-250 | within | 248 |
| core/crates/layerfs-content/src/file/mod.rs | Update | 10 | 17 | +7 | 14-24 | within | 23 |
| core/crates/layerfs-content/src/file/read.rs | Update | 111 | 111 | +0 | 90-160 | within | 127 |
| core/crates/layerfs-content/src/file/view.rs | New | 0 | 86 | +86 | 90-160 | below | 112 |
| core/crates/layerfs-content/src/lib.rs | Update | 16 | 24 | +8 | 18-35 | within | 41 |
| core/crates/layerfs-content/src/object/access.rs | Update | 18 | 18 | +0 | 40-90 | below | 36 |
| core/crates/layerfs-content/src/object/codec.rs | Update | 107 | 107 | +0 | 107-150 | within | 138 |
| core/crates/layerfs-content/src/object/id.rs | Retain | 89 | 77 | -12 | 89 | below | 105 |
| core/crates/layerfs-content/src/object/inode_leaf.rs | New | 0 | 307 | +307 | 160-260 | above | 392 |
| core/crates/layerfs-content/src/object/mod.rs | Update | 12 | 23 | +11 | 14-24 | within | 29 |
| core/crates/layerfs-content/src/object/output.rs | Update | 111 | 121 | +10 | 100-170 | within | 187 |
| core/crates/layerfs-content/src/object/predecessor.rs | New | 0 | 64 | +64 | 45-85 | within | 102 |
| core/crates/layerfs-content/src/policy.rs | Update | 106 | 135 | +29 | 150-250 | below | 226 |
| core/crates/layerfs-storage/sql/schema.sql | Update | 46 | 48 | +2 | 80-130 | below | 69 |
| core/crates/layerfs-storage/src/cas/batch.rs | Update | 58 | 58 | +0 | 90-160 | below | 87 |
| core/crates/layerfs-storage/src/cas/dependencies.rs | Update | 62 | 60 | -2 | 90-170 | below | 86 |
| core/crates/layerfs-storage/src/cas/finish.rs | Update | 16 | 16 | +0 | 20-50 | below | 25 |
| core/crates/layerfs-storage/src/cas/membership.rs | Update | 42 | 32 | -10 | 70-140 | below | 47 |
| core/crates/layerfs-storage/src/cas/mod.rs | Update | 11 | 11 | +0 | 14-24 | below | 16 |
| core/crates/layerfs-storage/src/cas/owner.rs | Update | 364 | 711 | +347 | 320-520 | above | 911 |
| core/crates/layerfs-storage/src/cas/read.rs | Update | 65 | 74 | +9 | 120-220 | below | 101 |
| core/crates/layerfs-storage/src/cas/save.rs | Update | 49 | 52 | +3 | 90-170 | below | 76 |
| core/crates/layerfs-storage/src/cas/store.rs | Update | 327 | 385 | +58 | 260-420 | within | 518 |
| core/crates/layerfs-storage/src/encoding/codec.rs | Retire | 367 | 508 | +141 | 0 | RETAINED, not retired | 633 |
| core/crates/layerfs-storage/src/encoding/codec/decode.rs | New | 0 | 0 | +0 | 260-420 | below | 0 |
| core/crates/layerfs-storage/src/encoding/codec/encode.rs | New | 0 | 0 | +0 | 220-340 | below | 0 |
| core/crates/layerfs-storage/src/encoding/codec/mod.rs | New | 0 | 0 | +0 | 8-16 | below | 0 |
| core/crates/layerfs-storage/src/encoding/codec/profile.rs | New | 0 | 0 | +0 | 80-140 | below | 0 |
| core/crates/layerfs-storage/src/encoding/decode.rs | Update | 103 | 170 | +67 | 100-180 | within | 192 |
| core/crates/layerfs-storage/src/encoding/delta/candidates.rs | New | 0 | 126 | +126 | 100-180 | within | 166 |
| core/crates/layerfs-storage/src/encoding/delta/mod.rs | New | 0 | 4 | +4 | 8-16 | below | 8 |
| core/crates/layerfs-storage/src/encoding/delta/read.rs | New | 0 | 224 | +224 | 170-300 | within | 298 |
| core/crates/layerfs-storage/src/encoding/delta/record.rs | New | 0 | 201 | +201 | 100-180 | above | 240 |
| core/crates/layerfs-storage/src/encoding/delta/select.rs | New | 0 | 276 | +276 | 160-280 | within | 380 |
| core/crates/layerfs-storage/src/encoding/full.rs | Update | 112 | 177 | +65 | 110-190 | within | 213 |
| core/crates/layerfs-storage/src/encoding/mod.rs | Update | 9 | 11 | +2 | 12-24 | below | 17 |
| core/crates/layerfs-storage/src/encoding/pool/delta.rs | New | 0 | 261 | +261 | 160-280 | within | 287 |
| core/crates/layerfs-storage/src/encoding/pool/index.rs | New | 0 | 203 | +203 | 160-280 | within | 261 |
| core/crates/layerfs-storage/src/encoding/pool/leaf.rs | New | 0 | 101 | +101 | 140-240 | below | 140 |
| core/crates/layerfs-storage/src/encoding/pool/mod.rs | New | 0 | 9 | +9 | 8-16 | within | 14 |
| core/crates/layerfs-storage/src/encoding/pool/read.rs | New | 0 | 309 | +309 | 160-280 | above | 366 |
| core/crates/layerfs-storage/src/encoding/pool/value_group.rs | New | 0 | 77 | +77 | 140-240 | below | 103 |
| core/crates/layerfs-storage/src/error.rs | Update | 105 | 105 | +0 | 120-200 | below | 154 |
| core/crates/layerfs-storage/src/lib.rs | Update | 11 | 11 | +0 | 14-28 | below | 30 |
| core/crates/layerfs-storage/src/pack/assemble.rs | Update | 195 | 223 | +28 | 180-300 | within | 252 |
| core/crates/layerfs-storage/src/pack/layout.rs | Update | 321 | 396 | +75 | 280-440 | within | 489 |
| core/crates/layerfs-storage/src/pack/mod.rs | Update | 10 | 13 | +3 | 14-24 | below | 18 |
| core/crates/layerfs-storage/src/pack/placement.rs | Update | 123 | 113 | -10 | 130-230 | below | 150 |
| core/crates/layerfs-storage/src/pack/read.rs | New | 0 | 0 | +0 | 100-190 | below | 0 |
| core/crates/layerfs-storage/src/pack/singleton.rs | New | 0 | 0 | +0 | 90-160 | below | 0 |
| core/crates/layerfs-storage/src/policy.rs | Update | 148 | 194 | +46 | 180-300 | within | 336 |
| core/crates/layerfs-storage/src/sqlite/cleanup.rs | Update | 58 | 70 | +12 | 80-150 | below | 100 |
| core/crates/layerfs-storage/src/sqlite/connection.rs | Update | 50 | 50 | +0 | 50-80 | within | 72 |
| core/crates/layerfs-storage/src/sqlite/lookup.rs | Update | 118 | 141 | +23 | 130-220 | within | 177 |
| core/crates/layerfs-storage/src/sqlite/mod.rs | Update | 8 | 10 | +2 | 10-20 | within | 15 |
| core/crates/layerfs-storage/src/sqlite/pool.rs | New | 0 | 118 | +118 | 120-200 | below | 162 |
| core/crates/layerfs-storage/src/sqlite/schema.rs | Update | 223 | 232 | +9 | 220-340 | within | 267 |
| core/crates/layerfs-storage/src/sqlite/write.rs | Update | 83 | 83 | +0 | 120-220 | below | 117 |
| core/crates/layerfs-telemetry/src/lib.rs | UNEXPECTED | 3 | 3 | +0 | (no plan row) | - | 16 |
| core/crates/layerfs-telemetry/src/timer/format.rs | UNEXPECTED | 69 | 69 | +0 | (no plan row) | - | 82 |
| core/crates/layerfs-telemetry/src/timer/json.rs | UNEXPECTED | 115 | 115 | +0 | (no plan row) | - | 136 |
| core/crates/layerfs-telemetry/src/timer/mod.rs | UNEXPECTED | 8 | 8 | +0 | (no plan row) | - | 25 |
| core/crates/layerfs-telemetry/src/timer/recording.rs | UNEXPECTED | 232 | 232 | +0 | (no plan row) | - | 291 |
| core/crates/layerfs-telemetry/src/timer/report.rs | UNEXPECTED | 170 | 170 | +0 | (no plan row) | - | 249 |
| core/crates/layerfs-telemetry/src/timer/scope.rs | UNEXPECTED | 135 | 135 | +0 | (no plan row) | - | 206 |

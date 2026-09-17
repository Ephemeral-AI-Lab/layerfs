
### C1 layerfs-content (production LOC)
path                                                                 pre1     pre5      rev       cum       s5    phys
core/crates/layerfs-content/src/error.rs                                0      120      153      +153      +33     219
core/crates/layerfs-content/src/file/cdc/gear.rs                        0      494      494      +494       +0     538
core/crates/layerfs-content/src/file/cdc/mod.rs                         0        5        5        +5       +0      10
core/crates/layerfs-content/src/file/content.rs                         0      197      197      +197       +0     254
core/crates/layerfs-content/src/file/edit/apply.rs                      0      389      390      +390       +1     455
core/crates/layerfs-content/src/file/edit/compare.rs                    0       67       68       +68       +1      87
core/crates/layerfs-content/src/file/edit/concat.rs                     0       19       19       +19       +0      29
core/crates/layerfs-content/src/file/edit/finish.rs                     0       38       38       +38       +0      54
core/crates/layerfs-content/src/file/edit/input.rs                      0      286      286      +286       +0     390
core/crates/layerfs-content/src/file/edit/mod.rs                        0       15       15       +15       +0      20
core/crates/layerfs-content/src/file/edit/split.rs                      0       23       23       +23       +0      32
core/crates/layerfs-content/src/file/edit/tree.rs                       0      707      707      +707       +0     913
core/crates/layerfs-content/src/file/mapping/build.rs                   0      309      309      +309       +0     391
core/crates/layerfs-content/src/file/mapping/codec.rs                   0      303      304      +304       +1     339
core/crates/layerfs-content/src/file/mapping/mod.rs                     0       15       17       +17       +2      22
core/crates/layerfs-content/src/file/mapping/read.rs                    0      223      268      +268      +45     327
core/crates/layerfs-content/src/file/mapping/types.rs                   0      185      190      +190       +5     266
core/crates/layerfs-content/src/file/mod.rs                             0       17       17       +17       +0      23
core/crates/layerfs-content/src/file/read.rs                            0      111      111      +111       +0     127
core/crates/layerfs-content/src/file/view.rs                            0       86       87       +87       +1     117
core/crates/layerfs-content/src/filesystem/attributes/build.rs          0        0      386      +386     +386     433
core/crates/layerfs-content/src/filesystem/attributes/codec.rs          0        0      322      +322     +322     375
core/crates/layerfs-content/src/filesystem/attributes/keys.rs           0        0       77       +77      +77     107
core/crates/layerfs-content/src/filesystem/attributes/mod.rs            0        0       16       +16      +16      19
core/crates/layerfs-content/src/filesystem/attributes/patch.rs          0        0      189      +189     +189     234
core/crates/layerfs-content/src/filesystem/attributes/portable.rs        0        0       70       +70      +70      95
core/crates/layerfs-content/src/filesystem/attributes/read.rs           0        0      167      +167     +167     196
core/crates/layerfs-content/src/filesystem/attributes/value.rs          0        0       70       +70      +70      88
core/crates/layerfs-content/src/filesystem/directory/codec.rs           0        0      156      +156     +156     190
core/crates/layerfs-content/src/filesystem/directory/mod.rs             0        0        6        +6       +6       9
core/crates/layerfs-content/src/filesystem/directory/read.rs            0        0      272      +272     +272     315
core/crates/layerfs-content/src/filesystem/directory/update.rs          0        0       39       +39      +39      52
core/crates/layerfs-content/src/filesystem/identity.rs                  0        0       32       +32      +32      63
core/crates/layerfs-content/src/filesystem/inode/codec.rs               0        0      145      +145     +145     178
core/crates/layerfs-content/src/filesystem/inode/mod.rs                 0        0        6        +6       +6       9
core/crates/layerfs-content/src/filesystem/inode/read.rs                0        0      186      +186     +186     211
core/crates/layerfs-content/src/filesystem/inode/update.rs              0        0       72       +72      +72      97
core/crates/layerfs-content/src/filesystem/input.rs                     0        0      158      +158     +158     232
core/crates/layerfs-content/src/filesystem/limits.rs                    0        0       23       +23      +23      66
core/crates/layerfs-content/src/filesystem/mod.rs                       0        0       28       +28      +28      33
core/crates/layerfs-content/src/filesystem/objects.rs                   0        0      102      +102     +102     167
core/crates/layerfs-content/src/filesystem/path.rs                      0        0      147      +147     +147     197
core/crates/layerfs-content/src/filesystem/read.rs                      0        0      212      +212     +212     275
core/crates/layerfs-content/src/filesystem/references/backing.rs        0        0      246      +246     +246     360
core/crates/layerfs-content/src/filesystem/references/merge.rs          0        0      154      +154     +154     204
core/crates/layerfs-content/src/filesystem/references/mod.rs            0        0       15       +15      +15      18
core/crates/layerfs-content/src/filesystem/references/record.rs         0        0      120      +120     +120     166
core/crates/layerfs-content/src/filesystem/references/reduce.rs         0        0      469      +469     +469     585
core/crates/layerfs-content/src/filesystem/references/release.rs        0        0      167      +167     +167     205
core/crates/layerfs-content/src/filesystem/references/runs.rs           0        0      417      +417     +417     586
core/crates/layerfs-content/src/filesystem/root.rs                      0        0      119      +119     +119     160
core/crates/layerfs-content/src/filesystem/sorted/budget.rs             0        0       87       +87      +87     117
core/crates/layerfs-content/src/filesystem/sorted/finish.rs             0        0      156      +156     +156     180
core/crates/layerfs-content/src/filesystem/sorted/format.rs             0        0      592      +592     +592     744
core/crates/layerfs-content/src/filesystem/sorted/merge.rs              0        0      282      +282     +282     314
core/crates/layerfs-content/src/filesystem/sorted/mod.rs                0        0        9        +9       +9      12
core/crates/layerfs-content/src/filesystem/sorted/page.rs               0        0      449      +449     +449     556
core/crates/layerfs-content/src/filesystem/symlink.rs                   0        0       70       +70      +70      94
core/crates/layerfs-content/src/filesystem/update.rs                    0        0      405      +405     +405     498
core/crates/layerfs-content/src/filesystem/validate.rs                  0        0      576      +576     +576     716
core/crates/layerfs-content/src/lib.rs                                  0       24       31       +31       +7      48
core/crates/layerfs-content/src/object/access.rs                        0       18       44       +44      +26      82
core/crates/layerfs-content/src/object/codec.rs                         0      107      107      +107       +0     138
core/crates/layerfs-content/src/object/id.rs                            0       77       70       +70       -7      96
core/crates/layerfs-content/src/object/inode_leaf.rs                    0      307      345      +345      +38     467
core/crates/layerfs-content/src/object/mod.rs                           0       23       23       +23       +0      29
core/crates/layerfs-content/src/object/output.rs                        0      121      142      +142      +21     214
core/crates/layerfs-content/src/object/predecessor.rs                   0       64       64       +64       +0     102
core/crates/layerfs-content/src/policy.rs                               0      137      137      +137       +0     237
TOTAL 69 files                                                          0     4487    11875    +11875    +7388   15182

### C2 layerfs-storage (production LOC, includes sql/schema.sql)
path                                                                 pre1     pre5      rev       cum       s5    phys
core/crates/layerfs-storage/sql/schema.sql                              0       48       48       +48       +0      72
core/crates/layerfs-storage/src/cas/batch.rs                            0       58       58       +58       +0      87
core/crates/layerfs-storage/src/cas/dependencies.rs                     0       60       60       +60       +0      86
core/crates/layerfs-storage/src/cas/finish.rs                           0       16       16       +16       +0      25
core/crates/layerfs-storage/src/cas/membership.rs                       0       32       32       +32       +0      47
core/crates/layerfs-storage/src/cas/mod.rs                              0       11       13       +13       +2      18
core/crates/layerfs-storage/src/cas/owner.rs                            0      725      728      +728       +3     957
core/crates/layerfs-storage/src/cas/provider.rs                         0        0       39       +39      +39      69
core/crates/layerfs-storage/src/cas/read.rs                             0       74       74       +74       +0     101
core/crates/layerfs-storage/src/cas/save.rs                             0       52       52       +52       +0      76
core/crates/layerfs-storage/src/cas/store.rs                            0      385      385      +385       +0     549
core/crates/layerfs-storage/src/encoding/codec.rs                       0      508      515      +515       +7     647
core/crates/layerfs-storage/src/encoding/decode.rs                      0      170      170      +170       +0     192
core/crates/layerfs-storage/src/encoding/delta/candidates.rs            0      126      126      +126       +0     166
core/crates/layerfs-storage/src/encoding/delta/mod.rs                   0        4        4        +4       +0       8
core/crates/layerfs-storage/src/encoding/delta/read.rs                  0      224      224      +224       +0     298
core/crates/layerfs-storage/src/encoding/delta/record.rs                0      201      201      +201       +0     240
core/crates/layerfs-storage/src/encoding/delta/select.rs                0      268      277      +277       +9     390
core/crates/layerfs-storage/src/encoding/full.rs                        0      178      185      +185       +7     225
core/crates/layerfs-storage/src/encoding/mod.rs                         0       11       11       +11       +0      17
core/crates/layerfs-storage/src/encoding/pool/delta.rs                  0      261      261      +261       +0     287
core/crates/layerfs-storage/src/encoding/pool/index.rs                  0      199      199      +199       +0     261
core/crates/layerfs-storage/src/encoding/pool/leaf.rs                   0      103      103      +103       +0     145
core/crates/layerfs-storage/src/encoding/pool/mod.rs                    0        9        9        +9       +0      14
core/crates/layerfs-storage/src/encoding/pool/read.rs                   0      309      309      +309       +0     366
core/crates/layerfs-storage/src/encoding/pool/value_group.rs            0       77       77       +77       +0     103
core/crates/layerfs-storage/src/error.rs                                0      105      105      +105       +0     154
core/crates/layerfs-storage/src/lib.rs                                  0       11       11       +11       +0      30
core/crates/layerfs-storage/src/pack/assemble.rs                        0      255      261      +261       +6     319
core/crates/layerfs-storage/src/pack/layout.rs                          0      390      397      +397       +7     488
core/crates/layerfs-storage/src/pack/mod.rs                             0       14       14       +14       +0      19
core/crates/layerfs-storage/src/pack/placement.rs                       0      119      119      +119       +0     162
core/crates/layerfs-storage/src/policy.rs                               0      187      191      +191       +4     344
core/crates/layerfs-storage/src/sqlite/cleanup.rs                       0      113      113      +113       +0     150
core/crates/layerfs-storage/src/sqlite/connection.rs                    0       50       50       +50       +0      72
core/crates/layerfs-storage/src/sqlite/lookup.rs                        0      141      138      +138       -3     172
core/crates/layerfs-storage/src/sqlite/mod.rs                           0       10       10       +10       +0      15
core/crates/layerfs-storage/src/sqlite/pool.rs                          0      122      122      +122       +0     177
core/crates/layerfs-storage/src/sqlite/schema.rs                        0      232      253      +253      +21     296
core/crates/layerfs-storage/src/sqlite/write.rs                         0       83       83       +83       +0     123
TOTAL 40 files                                                          0     5941     6043     +6043     +102    7967

### Directory rollups, core (recursive, children included once)
directory                                                files     pre1     pre5      rev       cum       s5
core                                                       116      732    11160    18650    +17918    +7490
core/crates                                                116      732    11160    18650    +17918    +7490
core/crates/layerfs-content                                 69        0     4487    11875    +11875    +7388
core/crates/layerfs-content/src                             69        0     4487    11875    +11875    +7388
core/crates/layerfs-storage                                 40        0     5941     6043     +6043     +102
core/crates/layerfs-storage/sql                              1        0       48       48       +48       +0
core/crates/layerfs-storage/src                             39        0     5893     5995     +5995     +102
core/crates/layerfs-telemetry                                7      732      732      732        +0       +0
core/crates/layerfs-telemetry/src                            7      732      732      732        +0       +0

### Reference scope
reference files 193 pre1 65417 pre5 65417 rev 65417 cum +0 s5 +0

### Combined
combined pre1 66149 pre5 76577 rev 84067 cum +17918 s5 +7490

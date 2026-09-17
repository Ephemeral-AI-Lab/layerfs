# Actual C1/C2 production tree at HEAD b29f8e4a3 (audited counter tools/production_loc.py)
# per file: before prod LOC -> after prod LOC, physical lines, [NEW] if absent in base

C1 core/crates/layerfs-content/src/
  src/                  4463 prod LOC / 29 files
    error.rs                       116 ->   120   phys  174
    file/                 3467 prod LOC / 19 files
      cdc/                   499 prod LOC / 2 files
        gear.rs                        494 ->   494   phys  538
        mod.rs                           5 ->     5   phys   10
      content.rs                     195 ->   197   phys  254
      edit/                 1538 prod LOC / 8 files
        apply.rs                         0 ->   389   phys  454  [NEW]
        compare.rs                       0 ->    67   phys   86  [NEW]
        concat.rs                        0 ->    19   phys   29  [NEW]
        finish.rs                        0 ->    38   phys   54  [NEW]
        input.rs                         0 ->   286   phys  390  [NEW]
        mod.rs                           0 ->    15   phys   20  [NEW]
        split.rs                         0 ->    23   phys   32  [NEW]
        tree.rs                          0 ->   701   phys  901  [NEW]
      mapping/              1019 prod LOC / 5 files
        build.rs                       236 ->   309   phys  391
        codec.rs                       303 ->   303   phys  334
        mod.rs                          15 ->    15   phys   20
        read.rs                        210 ->   210   phys  241
        types.rs                       182 ->   182   phys  248
      mod.rs                          10 ->    17   phys   23
      read.rs                        111 ->   111   phys  127
      view.rs                          0 ->    86   phys  112  [NEW]
    lib.rs                          16 ->    24   phys   41
    object/                717 prod LOC / 7 files
      access.rs                       18 ->    18   phys   36
      codec.rs                       107 ->   107   phys  138
      id.rs                           89 ->    77   phys  105
      inode_leaf.rs                    0 ->   307   phys  392  [NEW]
      mod.rs                          12 ->    23   phys   29
      output.rs                      111 ->   121   phys  187
      predecessor.rs                   0 ->    64   phys  102  [NEW]
    policy.rs                      106 ->   135   phys  226

C2 core/crates/layerfs-storage/
  sql/                    48 prod LOC / 1 files
    schema.sql                      46 ->    48   phys   69
  src/                  5815 prod LOC / 38 files
    cas/                  1399 prod LOC / 9 files
      batch.rs                        58 ->    58   phys   87
      dependencies.rs                 62 ->    60   phys   86
      finish.rs                       16 ->    16   phys   25
      membership.rs                   42 ->    32   phys   47
      mod.rs                          11 ->    11   phys   16
      owner.rs                       364 ->   711   phys  911
      read.rs                         65 ->    74   phys  101
      save.rs                         49 ->    52   phys   76
      store.rs                       327 ->   385   phys  518
    encoding/             2657 prod LOC / 15 files
      codec.rs                       367 ->   508   phys  633
      decode.rs                      103 ->   170   phys  192
      delta/                 831 prod LOC / 5 files
        candidates.rs                    0 ->   126   phys  166  [NEW]
        mod.rs                           0 ->     4   phys    8  [NEW]
        read.rs                          0 ->   224   phys  298  [NEW]
        record.rs                        0 ->   201   phys  240  [NEW]
        select.rs                        0 ->   276   phys  380  [NEW]
      full.rs                        112 ->   177   phys  213
      mod.rs                           9 ->    11   phys   17
      pool/                  960 prod LOC / 6 files
        delta.rs                         0 ->   261   phys  287  [NEW]
        index.rs                         0 ->   203   phys  261  [NEW]
        leaf.rs                          0 ->   101   phys  140  [NEW]
        mod.rs                           0 ->     9   phys   14  [NEW]
        read.rs                          0 ->   309   phys  366  [NEW]
        value_group.rs                   0 ->    77   phys  103  [NEW]
    error.rs                       105 ->   105   phys  154
    lib.rs                          11 ->    11   phys   30
    pack/                  745 prod LOC / 4 files
      assemble.rs                    195 ->   223   phys  252
      layout.rs                      321 ->   396   phys  489
      mod.rs                          10 ->    13   phys   18
      placement.rs                   123 ->   113   phys  150
    policy.rs                      148 ->   194   phys  336
    sqlite/                704 prod LOC / 7 files
      cleanup.rs                      58 ->    70   phys  100
      connection.rs                   50 ->    50   phys   72
      lookup.rs                      118 ->   141   phys  177
      mod.rs                           8 ->    10   phys   15
      pool.rs                          0 ->   118   phys  162  [NEW]
      schema.rs                      223 ->   232   phys  267
      write.rs                        83 ->    83   phys  117

Telemetry  core/crates/layerfs-telemetry/src/   732 prod LOC / 7 files (pre-existing, unchanged this batch)
Reference  crates/                             65417 prod LOC / 193 files (unchanged coexistence)

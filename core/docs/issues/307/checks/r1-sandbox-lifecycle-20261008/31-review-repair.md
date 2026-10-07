# Review and original test repair

Read-only review identified a real borrowed-volume mutation: including layerfs-store/global/ in each tar would rewrite shared parent metadata. Removed that entry. Installer-mode daemon config already creates/checks missing parent0700 once; existing-store mode checks it without create. Tar now writes only owned container-local config/Overlay/mount/executable paths. Final test rejects any shared Store entry and validates every ustar checksum.

29 success test FAILED a test oracle slice: configuration path is33 bytes but manual slice selected32. Replaced magic endpoint with literal byte length. Product tar bytes were correct. Original raw failure retained. Archive disposition and actual proof rerun at final source identity after review repair, without changing wait/workload limits.

22 all-target build FAILED example WorkspaceId used16 rather than required32 authority bytes and omitted Result extraction. Repaired exact public signature. 24/25 and27/28 PASS. These failures are source/test preparation failures, not a native Store or FUSE verdict.

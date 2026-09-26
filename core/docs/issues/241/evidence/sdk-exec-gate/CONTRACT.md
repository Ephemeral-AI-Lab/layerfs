# #241 public SDK Exec→Commit gate: one functional selection

This test-only selection uses the frozen [Linux ABI](../../RANGE_IOCTL_ABI.md),
the published product FUSE adapter, and the one reusable
`/layerfs-bench/bin/layerfs-edit-tool splice` command. It is not a benchmark
speed arm or a cold-cache claim. One live attempt at one source/image identity;
retain any failure without rerunning it to pass. Source fixture is a fresh
8,192-byte `data.bin` (`byte[i]=i%251`) plus a 4,096-byte `payload.bin`
(`byte[i]=i%239`), created before mount through public `ProjectApi::init`.
This small functional fixture does not cover the separate large-file or
position-sweep selections.

1. Public SDK `SandboxApi::create` and `WorkspaceApi::mount` attach a writable
   real Linux FUSE Workspace. Public `WorkspaceApi::exec` invokes exactly:

   ```sh
   /layerfs-bench/bin/layerfs-edit-tool splice --file data.bin --expect-size 8192 --offset 4093 --delete-length 0 --length 4096 --payload payload.bin
   ```

2. The tool itself must issue STATE, EDIT and second STATE on the same open
   descriptor, then match `fstat` and bounded boundary readback. The SDK gate
   calls explicit `WorkspaceApi::commit` **only** when the returned
   `ExecResult.exit_status` is `Some(0)`. Required observations: untruncated
   JSON `PASS`, final length 12,288, shifted bytes zero, one range-edit
   callback, at least two range-state callbacks, accepted payload bytes
   4,096, shifted suffix bytes zero, and a `Committed` outcome.

3. In the same mounted Workspace, a separate public Exec of `exit 75`
   exercises the SDK's UNKNOWN gate. Required `ExecResult.exit_status` is
   `Some(75)` and Commit is absent. This is a **synthetic process exit** to
   prove the caller branch; it is not a real failed range ioctl or a product
   loss-of-reply proof. A pure test also rejects `None` and other nonzero
   statuses. The mounted test must unmount and delete through SDK APIs.

The complete command wall, stdout/stderr, binary/image/source identities,
mount and cleanup observations will be retained. Instruction/kernel cache
state is uncontrolled, so the result supports only functional delivery and
caller gating. The product's actual post-EDIT exit-75 path, large fixtures,
old/new Commit oracle, and Edit→Commit performance remain separate work.

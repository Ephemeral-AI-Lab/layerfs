# Sandbox runtime providers

Status: implemented in H20 on the #427 macOS native base; no Linux native or Docker provider qualification is claimed.

`layerfs-sandbox::provider` defines object-safe `SandboxProvider` and `SandboxInstance` ports, capabilities, launch inputs, exact partial launch ownership and non-HTTP error construction. `ExecRequest` lives in the shared types layer; its Docker re-export remains compatible. `SandboxId` aliases the existing `ContainerId`, preserving encoded identities and records. The runtime layer has no daemon control dependency.

`NativeMacosProvider::new(root)` wraps the existing local runtime and Darwin process facts. It refuses other platforms before root access. Launch preserves the original single-attempt process lifecycle, native identity, inherited PATH, NFS environment and partial owner on failure. `inspect` reports exact process liveness; unknown exit/storage facts stay absent. Records remain the Node binding’s responsibility; providers neither scan directories nor infer project ownership. B8 list aggregation and doctor remain future work.

`SandboxApi::with_provider(Arc<dyn SandboxProvider>)` and `create_provider_file(ProviderCreate, store, manifest)` launch a provider and return `ProviderSandbox`. Before launch, API validates authentication identities, selected Store, command identity and local resource ownership: overlay and mount home must belong to the instance, Store must live outside it, and native listen uses dynamic loopback. API encodes daemon configuration, then uses the shared TCP/Noise/Hello and Store-install pipeline in `sandbox/connection.rs`. Failures retain their phase, original cause, runtime, control, observed status, installation acknowledgement and handshake work. No automatic retry or cleanup occurs in API.

The macOS Node binding uses this path, transfers the original control owner with `internal::split_provider`, and retains its existing native records and explicit cleanup policy. Local command execution delegates through `Box<dyn SandboxInstance>`; record reopen still validates the existing directory/PID/start/boot/port fields and wraps the same local instance. No record migration is required.

For compatibility, Docker callers retain `SandboxApi::new(Docker)`, `SandboxCreate`, `ManagedSandbox`, `internal::split/parts/exec` and ordinary streaming Exec handles. This thin legacy path shares authentication, Hello and install with providers. `internal::split_provider/provider_parts` provide the equivalent instance/control access for providers. Fully adapting Docker and unifying its public runtime/Exec handles is deferred; there is no new Docker launch option pretending to implement it.

The Store remains explicitly Disposable/WAL/OFF. Native macOS keeps #427’s direct Store access without a worker; this refactor does not change persistence authorization or filesystem semantics.

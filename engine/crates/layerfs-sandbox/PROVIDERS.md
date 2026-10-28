# Sandbox Docker provider

Status: Current general guide.

`backend::docker::DockerProvider::new(docker, deployment)` adapts the existing
Engine client and `SandboxRequest`. `DockerInstance` aliases the original
`docker::Sandbox`: the provider shares its lifecycle admission, API 1.48–1.54
negotiation cache, streamed private archive, logs, exact deployment inspection,
and explicit stop/delete. It introduces no automatic cleanup or retries.

Import `provider::{SandboxProvider, SandboxInstance}` to use the ports. Call
`provider.launch(LaunchRequest { .. })` with the executable, private configuration,
name, matching deployment labels and explicit startup wait. The current shared
launch options contain only `ProviderLaunchOptions::Local`: Docker checks its
command identity and requires an empty environment. Image, volume and daemon port
come from the constructor's `SandboxRequest`. This temporary naming preserves
existing API consumers without changing the trait or option enum in this slice.
The API integration follow-up can introduce appropriate common launch options.
The runtime does not decode daemon configuration or perform authentication.

The instance supports `endpoint`, `inspect`, `run`, `log_tail`, `stop` and
`delete`. `run` streams stdout/stderr to caller sinks with bounded windows and
returns `ExecExit` only after one inspection knows the root process exit. It
rejects `stdin=true` before effects: interactive users keep the existing concrete
`create_exec/start/into_parts` stream API. EOF alone is not an exit. No automatic
poll follows an unknown exit. `track` needs no host registration because container
teardown owns descendants; scratch is `/tmp` and `local_record()` is absent.
`inspect` checks the recorded deployment but neither adopts it nor changes
lifecycle admission; unknown PID/exit/storage fields remain absent.

A `LaunchFailure` retains the acknowledged instance, when any. Its cause may be
`RuntimeError::Docker(Box<docker::ProviderFailure>)`: match the typed variant for
the original Create/Upload/Start/Listener receipt, partial IDs, upload work and
pending source bytes. Upload is within existing `LaunchPhase::Create`; inspect
the typed cause to distinguish it. A partially decoded Create ID is never adopted.

For `run` failures, the outer `WireFailure` summarizes the deciding exchange's
scalar transfer facts when one exists. Output/split/unknown-exit errors mark an
attempt but do not invent a new HTTP exchange. Original pending buffers,
diagnostics, fence errors and Exec owners remain in the nested `ProviderFailure`.
`ExecInspect` and `ExecExitUnknown` also retain delivered `OutputProgress`.
Retaining a handle permits an explicit later observation; it never permits replay
of creation or Start. Ordinary commands have no automatic runtime timeout.

Records remain a caller responsibility. `reopen_docker(id, SandboxRequest)`
validates the exact existing deployment and endpoint without starting it.
`reopen_stopped_docker` requires the caller's prior exact stopped observation and
admits only non-forcing Delete. The existing trait reopen methods accept only
`LocalRecord`; Docker refuses those before effects. No disk format changes occur.

This slice does not route `SandboxApi` or the Node binding through the provider.
Their existing Docker paths remain for the follow-up, which must validate daemon
topology and preserve unknown Create/install custody before cleanup. B8/B9/B10
are outside this slice. Protocol peers prove requests and ownership contracts;
real Engine/FUSE/SDK qualification requires a Linux host separately.

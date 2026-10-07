# R1a independent correctness review and focused correction

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Read-only public-API reviewer found no second exchange, automatic retry or native
Ready claim. Relocated control and Init/install type modules are unchanged.
OperationFailure preserves Request and original boxed ControlFailure/typed
correlated refusal. Source review found two scoped proof/admission gaps.

1. SealedProject publicly supplies both SealedStore.profile and
StoreManifest.profile. The new host install admission checked only the manifest,
while SealedStore documents its profile as the one used on open. Their source
construction normally agrees, but a supplied/modified public value could
contradict before-effect admission. Dataflow traced from Persistence seal → SDK
Init metadata → SDK install: require both explicit Disposable before File::open
or any send. One source condition is added. Disabled-profile runtime reproduction
is NOT_RUN under the owner's prohibition; no Durable Store/test/diagnostic is
executed. Supported Disposable install is covered by the real installed-Store
control fixture. Retained Persistence Durable API/implementation is not removed.

2. The original lost-reply fixture still exercised only low-level Control on its
failing side. Adapt that existing real channel-loss test to WorkspaceApi.commit,
assert outer selected Request equals inner original Call.request, and retain
attempted/received/fence uncertainty through a separate observer. This exercises
actual facade custody, with no new synthetic hook/fixture or automatic replay.

The systematic-debugging workflow was read before the correction. Cause and
successful metadata pattern were traced in source; no secret diagnostic
instrumentation, disabled profile execution or new numerical gate is added.
Earlier successful build/test/check receipts are retained unchanged. The profile
condition changes product identity, so final affected builds/proofs/checks run
once at that final identity; this is not an unchanged treatment resample.

Separate native review remains prospective R1b guidance: transfer the actual
InstalledStore.opened owner to serving without reopen; initialize Overlay once;
distinguish daemon ControlReady from FUSE Ready; reject unavailable namespace
Commit before capture; protect daemon credentials outside container env/argv;
ordinary Docker client kill is not remote cancellation. Actual Sandbox/native
composition is still required before R1 completes.

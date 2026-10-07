//! One authority-bound upstream and initialized overlay owner, no whole-Save checkout.
use super::{
    binding, AttachPhase, AttachRefusal, AttachSuccess, BindingMismatch, ExpectedBinding,
    PersistenceBootstrap, UpstreamError,
};
use crate::{Command, OwnerClient, OwnerError, Response};
use layerfs_content::filesystem::{FilesystemRootId, InodeScope};
use layerfs_sdk::client::{
    Attachment, Calls, ClientRequest, Operation, RemoteBinding, RemoteLengths, RemoteObjects,
    ReplyView,
};
use layerfs_workspace::{BaseView, CanonicalCache, CanonicalClient, Workspace};
use std::sync::Arc;

/// One provisioned host/Workspace authority context and its bounded shared cache.
/// Every ordinary operation uses a fresh operation provider/scoped Workspace.
pub struct Upstream {
    pub(super) attachment: Attachment,
    pub(super) calls: Arc<Calls>,
    pub(super) binding: RemoteBinding,
    pub(super) expected: ExpectedBinding,
    pub(super) bootstrap: PersistenceBootstrap,
    pub(super) cache: Arc<CanonicalCache>,
    pub(super) workspace: Arc<Workspace>,
    pub(super) owner: OwnerClient,
}
impl Upstream {
    /// Requests original Binding and Policy once, validates the provisioned
    /// context, constructs a checked base before one local Open, then binds only
    /// a known Opened result. Every refusal returns original attachment/custody.
    /// The caller initialized Owner and host Runtime/Store before this operation.
    #[allow(clippy::result_large_err)]
    pub fn attach(
        attachment: Attachment,
        expected: ExpectedBinding,
        bootstrap: PersistenceBootstrap,
        owner: OwnerClient,
        cache_bytes: usize,
    ) -> Result<Self, AttachRefusal> {
        Self::attach_with_receipts(attachment, expected, bootstrap, owner, cache_bytes)
            .map(|attached| attached.upstream)
    }
    /// Performs the same single attachment attempt while returning original
    /// known Open and Binding/Policy replies instead of releasing those receipts.
    /// Failure follows the identical first-cause/custody path as `attach`.
    #[allow(clippy::result_large_err)]
    pub fn attach_with_receipts(
        attachment: Attachment,
        expected: ExpectedBinding,
        bootstrap: PersistenceBootstrap,
        owner: OwnerClient,
        cache_bytes: usize,
    ) -> Result<AttachSuccess, AttachRefusal> {
        let calls = attachment.calls();
        let mut phase = AttachPhase::Provision;
        let mut binding_reply = None;
        let mut policy_reply = None;
        let mut objects = None;
        let result = (|| {
            binding::provision(&expected, &bootstrap, calls.peer().public_key())?;
            phase = AttachPhase::Binding;
            let request = ClientRequest::control(Operation::Binding, None, 0)
                .map_err(UpstreamError::Frame)?;
            binding_reply = Some(
                calls
                    .call(request)
                    .map_err(|e| UpstreamError::Call(Box::new(e)))?,
            );
            let binding = match ReplyView::decode(
                binding_reply
                    .as_ref()
                    .expect("original Binding reply")
                    .bytes(),
            )
            .map_err(UpstreamError::Frame)?
            {
                ReplyView::Binding(binding) => binding,
                _ => return Err(UpstreamError::Mismatch(BindingMismatch::ReplyShape)),
            };
            binding::received(&expected, &binding)?;
            phase = AttachPhase::Policy;
            let request =
                ClientRequest::control(Operation::Policy, None, 0).map_err(UpstreamError::Frame)?;
            policy_reply = Some(
                calls
                    .call(request)
                    .map_err(|e| UpstreamError::Call(Box::new(e)))?,
            );
            let policy = match ReplyView::decode(
                policy_reply
                    .as_ref()
                    .expect("original Policy reply")
                    .bytes(),
            )
            .map_err(UpstreamError::Frame)?
            {
                ReplyView::Policy(policy) => policy,
                _ => return Err(UpstreamError::Mismatch(BindingMismatch::ReplyShape)),
            };
            if policy != expected.policy {
                return Err(UpstreamError::Mismatch(BindingMismatch::Policy));
            }
            phase = AttachPhase::Base;
            let cache = Arc::new(CanonicalCache::new(cache_bytes));
            let source = Arc::new(RemoteObjects::new(calls.clone(), None));
            objects = Some(source.clone());
            let lengths = Arc::new(RemoteLengths::new(calls.clone()));
            let client = Arc::new(CanonicalClient::with_cache(
                source,
                Some(lengths),
                cache.clone(),
            ));
            let base = BaseView::open(
                client,
                FilesystemRootId(expected.snapshot.effective_root),
                InodeScope::from_object(expected.snapshot.scope),
            )
            .map_err(UpstreamError::Content)?;
            if base.root().root_inode().serial() != expected.root_serial {
                return Err(UpstreamError::Mismatch(BindingMismatch::RootSerial));
            }
            phase = AttachPhase::Open;
            let pending = owner
                .try_submit(
                    None,
                    Command::Open {
                        incarnation: expected.workspace.to_bytes(),
                        base_root: expected.snapshot.effective_root.to_bytes(),
                    },
                )
                .map_err(|(cause, command)| {
                    UpstreamError::Owner(OwnerError::Unattempted {
                        cause: Box::new(cause),
                        command: Box::new(command),
                    })
                })?;
            let completion = pending.wait().map_err(UpstreamError::Owner)?;
            let route = match completion.result() {
                Ok(Response::Opened(route)) => *route,
                _ => return Err(UpstreamError::Completion(Box::new(completion))),
            };
            Ok((
                binding,
                cache,
                Arc::new(Workspace::bind(route, base)),
                completion,
            ))
        })();
        match result {
            Ok((binding, cache, workspace, open)) => Ok(AttachSuccess {
                upstream: Self {
                    attachment,
                    calls,
                    binding,
                    expected,
                    bootstrap,
                    cache,
                    workspace,
                    owner,
                },
                open,
                binding_reply: binding_reply.expect("known original Binding reply"),
                policy_reply: policy_reply.expect("known original Policy reply"),
            }),
            Err(error) => Err(AttachRefusal {
                phase,
                error,
                attachment,
                expected,
                bootstrap,
                binding_reply,
                policy_reply,
                objects,
            }),
        }
    }
    /// Exact validated original wire binding; no refresh occurs.
    pub fn binding(&self) -> &RemoteBinding {
        &self.binding
    }
    /// Original requested context and policy.
    pub fn expected(&self) -> &ExpectedBinding {
        &self.expected
    }
    /// Independently trusted actual host persistence profile/context.
    pub const fn persistence(&self) -> PersistenceBootstrap {
        self.bootstrap
    }
    /// Known locally opened route, independent of native FUSE mount readiness.
    pub fn route(&self) -> layerfs_overlay::Route {
        self.workspace.route()
    }
    /// Shared immutable-cache observations inside this authority context only.
    pub fn cache_work(&self) -> layerfs_content::ContentResult<layerfs_workspace::ClientWork> {
        self.cache.diagnostics()
    }
    /// Existing local owner ports and explicit lifecycle commands.
    pub fn owner(&self) -> OwnerClient {
        self.owner.clone()
    }
    /// Starts explicit socket fencing without aborting/closing a Workspace.
    pub fn fence(&mut self) {
        self.attachment.fence();
    }
    /// Original consumer completion fence after all bounded calls released their driver.
    pub fn try_join(
        &mut self,
    ) -> Result<Option<layerfs_sdk::client::ConsumerFence>, layerfs_bridge::contract::FrameError>
    {
        self.attachment.try_join()
    }
}

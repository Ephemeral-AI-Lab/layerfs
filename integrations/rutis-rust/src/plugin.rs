//! The rutis plugin wrapper: validates configuration, provides one `LayerFs`.

use crate::config::Config;
use crate::store::LayerFs;
use rutis::{BoxFuture, CordisError, Ctx, Effect, Plugin};

/// A built-in Rust plugin over one existing LayerFS project.
pub struct LayerFsPlugin {
    config: Config,
}

impl LayerFsPlugin {
    /// Creates the plugin from one project's configuration.
    pub fn new(config: Config) -> Self {
        Self { config }
    }
}

impl Plugin for LayerFsPlugin {
    fn name(&self) -> &str {
        "layerfs"
    }

    fn apply<'a>(&'a self, ctx: &'a Ctx) -> BoxFuture<'a, Result<Effect, CordisError>> {
        Box::pin(async move {
            let layerfs = LayerFs::new(self.config.clone())
                .map_err(|error| CordisError::PluginFailed(Box::new(error)))?;
            let cleanup = layerfs.clone();
            ctx.provide(layerfs)?;
            Ok(Effect::AsyncDisposer(Box::new(move || {
                Box::pin(async move {
                    cleanup
                        .close()
                        .await
                        .map_err(|error| CordisError::PluginFailed(Box::new(error)))
                })
            })))
        })
    }
}

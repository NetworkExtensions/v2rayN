use crate::{
    config_store::ConfigStore,
    models::CoreAssetStatus,
    core_runtime::RuntimeManager,
    core_update::CorePaths,
};
use std::sync::Mutex;

#[derive(Debug)]
pub struct SharedState {
    pub store: ConfigStore,
    pub core_paths: CorePaths,
    pub runtime: RuntimeManager,
    pub core_status_cache: Mutex<Vec<CoreAssetStatus>>,
    pub subscription_refresh_lock: Mutex<()>,
}

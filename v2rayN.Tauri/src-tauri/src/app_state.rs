use crate::{
    config_store::ConfigStore,
    core_runtime::RuntimeManager,
    core_update::CorePaths,
};
use std::sync::Mutex;

#[derive(Debug)]
pub struct SharedState {
    pub store: ConfigStore,
    pub core_paths: CorePaths,
    pub runtime: RuntimeManager,
    pub subscription_refresh_lock: Mutex<()>,
}

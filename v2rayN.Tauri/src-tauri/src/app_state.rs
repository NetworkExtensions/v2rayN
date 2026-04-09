use crate::{
    config_store::ConfigStore,
    core_runtime::RuntimeManager,
    core_update::CorePaths,
    events::EventSender,
    models::CoreAssetStatus,
};
use std::sync::{Arc, Mutex};

/// 全局共享状态。所有字段均可跨线程安全访问。
/// 实现 Clone：Arc 字段只增加引用计数，不复制底层数据。
#[derive(Clone, Debug)]
pub struct SharedState {
    pub store: ConfigStore,
    pub core_paths: CorePaths,
    pub runtime: Arc<RuntimeManager>,
    pub core_status_cache: Arc<Mutex<Vec<CoreAssetStatus>>>,
    pub subscription_refresh_lock: Arc<Mutex<()>>,
    pub event_sender: EventSender,
}

/// 独立 HTTP 服务器 —— 无需 Tauri 窗口即可在浏览器中控制代理核心。
///
/// 用法：
///   cargo run --bin server              # 开发模式
///   cargo run --bin server -- --port 8080   # 自定义端口
///
/// 然后在浏览器中打开 npm run dev 的地址（默认 http://localhost:1420）。
use app_lib::{
    app_state::SharedState,
    config_store::ConfigStore,
    core_runtime::RuntimeManager,
    core_update::CorePaths,
    events::EventSender,
    http_server,
    commands,
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

fn data_dir() -> PathBuf {
    if let Some(dir) = dirs::data_local_dir() {
        dir.join("com.dywang.v2rayn.tauri")
    } else {
        PathBuf::from("./data")
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .init();

    let mut port: u16 = 7393;
    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "--port") {
        if let Some(p) = args.get(pos + 1).and_then(|v| v.parse().ok()) {
            port = p;
        }
    }

    let base = data_dir();
    log::info!("数据目录: {}", base.display());

    let store = ConfigStore::bootstrap_at(base.clone())
        .expect("初始化 ConfigStore 失败");
    let core_paths = CorePaths {
        bin_root: PathBuf::from(store.paths().bin.clone()),
    };
    let core_status_cache =
        app_lib::core_update::list_local_core_statuses(&core_paths).unwrap_or_default();

    let event_sender = EventSender::new();

    let state = SharedState {
        store,
        core_paths,
        runtime: Arc::new(RuntimeManager::new()),
        core_status_cache: Arc::new(Mutex::new(core_status_cache)),
        subscription_refresh_lock: Arc::new(Mutex::new(())),
        event_sender: event_sender.clone(),
    };

    // 后台定时刷新订阅
    let sub_state = state.clone();
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(60));
        match commands::auto_refresh_due_subscriptions(&sub_state) {
            Ok(true) => sub_state.event_sender.emit_app_state_changed("subscription_auto_refresh"),
            Ok(false) => {}
            Err(e) => log::warn!("自动刷新订阅失败: {e}"),
        }
    });

    // 后台定时刷新核心状态
    let core_state = state.clone();
    thread::spawn(move || loop {
        match app_lib::core_update::list_core_statuses(&core_state.core_paths) {
            Ok(statuses) => {
                if let Ok(mut cache) = core_state.core_status_cache.lock() {
                    *cache = statuses;
                }
                core_state.event_sender.emit_app_state_changed("core_status_cache_updated");
            }
            Err(e) => log::warn!("刷新核心状态缓存失败: {e}"),
        }
        thread::sleep(Duration::from_secs(1800));
    });

    log::info!("HTTP 服务器将在 http://127.0.0.1:{port} 启动");
    log::info!("请确保前端 (npm run dev) 也已启动，然后用浏览器访问 http://localhost:1420");

    let rt = tokio::runtime::Runtime::new().expect("创建 Tokio 运行时失败");
    rt.block_on(http_server::serve(state, port));
}

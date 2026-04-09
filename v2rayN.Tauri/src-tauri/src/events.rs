use crate::models::{BackgroundTaskEvent, CoreLogEvent};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::broadcast;

/// 统一的应用事件，通过广播频道分发给 Tauri 前端和 HTTP WebSocket 客户端。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", content = "data", rename_all = "kebab-case")]
pub enum AppEvent {
    CoreLog(CoreLogEvent),
    AppStateChanged(String),
    BackgroundTaskFinished(BackgroundTaskEvent),
}

/// 线程安全、可克隆的事件发送器。内部持有 Arc<Sender>，克隆只增加引用计数。
#[derive(Clone, Debug)]
pub struct EventSender {
    tx: Arc<broadcast::Sender<AppEvent>>,
}

impl EventSender {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(512);
        Self { tx: Arc::new(tx) }
    }

    /// 订阅事件流，返回 Receiver。支持多个独立消费者（Tauri 桥接、WS 客户端等）。
    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.tx.subscribe()
    }

    pub fn emit_core_log(&self, event: CoreLogEvent) {
        let _ = self.tx.send(AppEvent::CoreLog(event));
    }

    pub fn emit_app_state_changed(&self, reason: impl Into<String>) {
        let _ = self.tx.send(AppEvent::AppStateChanged(reason.into()));
    }

    pub fn emit_background_task(&self, event: BackgroundTaskEvent) {
        let _ = self.tx.send(AppEvent::BackgroundTaskFinished(event));
    }
}

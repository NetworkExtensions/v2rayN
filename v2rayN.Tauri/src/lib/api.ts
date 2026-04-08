import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

import { browserApi, isTauriRuntime } from './browserApi'
import type {
  AppConfig,
  AppStatus,
  CoreAssetStatus,
  CoreLogEvent,
  CoreType,
  RunningStatus,
  Subscription,
} from './types'

const tauriApi = {
  getStatus: () => invoke<AppStatus>('get_app_status'),
  saveConfig: (config: AppConfig) => invoke<AppConfig>('save_app_config', { config }),
  importShareLinks: (raw: string, coreType: CoreType) =>
    invoke<AppConfig>('import_share_links', { raw, coreType }),
  saveSubscription: (subscription: Subscription) =>
    invoke<AppConfig>('save_subscription', { subscription }),
  refreshSubscription: (subscriptionId: string, coreType: CoreType) =>
    invoke<AppConfig>('refresh_subscription', { subscriptionId, coreType }),
  generatePreview: () => invoke<string>('generate_config_preview'),
  checkCoreAssets: () => invoke<CoreAssetStatus[]>('check_core_assets'),
  downloadCoreAsset: (coreType: CoreType) =>
    invoke<CoreAssetStatus>('download_core_asset', { coreType }),
  startCore: () => invoke<RunningStatus>('start_core'),
  stopCore: () => invoke<RunningStatus>('stop_core'),
  enableSystemProxy: () => invoke<AppConfig>('enable_system_proxy'),
  disableSystemProxy: () => invoke<AppConfig>('disable_system_proxy'),
  onCoreLog: (handler: (event: CoreLogEvent) => void) =>
    listen<CoreLogEvent>('core-log', ({ payload }) => handler(payload)),
}

export const desktopApi = isTauriRuntime() ? tauriApi : browserApi
export const runtimeMode = isTauriRuntime() ? 'tauri' : 'browser'

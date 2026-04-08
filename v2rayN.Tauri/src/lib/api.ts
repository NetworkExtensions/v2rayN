import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

import type {
  AppConfig,
  AppStatus,
  ClashConnection,
  ClashProxyGroup,
  ClashProxyProvider,
  CoreAssetStatus,
  CoreLogEvent,
  CoreType,
  ImportPreview,
  ProxyProbe,
  RoutingItem,
  RunningStatus,
  Subscription,
} from './types'

export const desktopApi = {
  getStatus: () => invoke<AppStatus>('get_app_status'),
  saveConfig: (config: AppConfig) => invoke<AppConfig>('save_app_config', { config }),
  initializeBuiltinRouting: (advancedOnly = false) =>
    invoke<AppConfig>('initialize_builtin_routing', { advancedOnly }),
  importRoutingTemplateUrl: (url: string, advancedOnly = false) =>
    invoke<AppConfig>('import_routing_template_url', { url, advancedOnly }),
  saveRoutingItem: (routingItem: RoutingItem) => invoke<AppConfig>('save_routing_item', { routingItem }),
  removeRoutingItem: (routingId: string) => invoke<AppConfig>('remove_routing_item', { routingId }),
  setDefaultRoutingItem: (routingId: string) =>
    invoke<AppConfig>('set_default_routing_item', { routingId }),
  importRoutingRules: (routingId: string, raw: string, replaceExisting = false) =>
    invoke<AppConfig>('import_routing_rules', { routingId, raw, replaceExisting }),
  exportRoutingRules: (routingId: string, ruleIds?: string[]) =>
    invoke<string>('export_routing_rules', { routingId, ruleIds }),
  moveRoutingRule: (routingId: string, ruleId: string, direction: string) =>
    invoke<AppConfig>('move_routing_rule', { routingId, ruleId, direction }),
  importShareLinks: (raw: string, coreType: CoreType) =>
    invoke<AppConfig>('import_share_links', { raw, coreType }),
  previewImport: (raw: string, coreType: CoreType) =>
    invoke<ImportPreview>('preview_import_result', { raw, coreType }),
  importFullConfig: (raw: string) => invoke<AppConfig>('import_full_config', { raw }),
  saveSubscription: (subscription: Subscription) =>
    invoke<AppConfig>('save_subscription', { subscription }),
  removeSubscription: (subscriptionId: string) =>
    invoke<AppConfig>('remove_subscription', { subscriptionId }),
  refreshSubscription: (subscriptionId: string, coreType: CoreType) =>
    invoke<AppConfig>('refresh_subscription', { subscriptionId, coreType }),
  refreshAllSubscriptions: (coreType: CoreType) =>
    invoke<AppConfig>('refresh_all_subscriptions', { coreType }),
  removeProfile: (profileId: string) => invoke<AppConfig>('remove_profile', { profileId }),
  selectProfile: (profileId: string) => invoke<AppConfig>('select_profile', { profileId }),
  generatePreview: () => invoke<string>('generate_config_preview'),
  checkCoreAssets: () => invoke<CoreAssetStatus[]>('check_core_assets'),
  downloadCoreAsset: (coreType: CoreType) =>
    invoke<CoreAssetStatus>('download_core_asset', { coreType }),
  startCore: () => invoke<RunningStatus>('start_core'),
  stopCore: () => invoke<RunningStatus>('stop_core'),
  restartCore: () => invoke<RunningStatus>('restart_core'),
  enableSystemProxy: () => invoke<AppConfig>('enable_system_proxy'),
  disableSystemProxy: () => invoke<AppConfig>('disable_system_proxy'),
  probeCurrentOutbound: () => invoke<ProxyProbe>('probe_current_outbound'),
  getClashProxyGroups: () => invoke<ClashProxyGroup[]>('get_clash_proxy_groups'),
  getClashProxyProviders: () => invoke<ClashProxyProvider[]>('get_clash_proxy_providers'),
  selectClashProxy: (groupName: string, proxyName: string) =>
    invoke<void>('select_clash_proxy', { groupName, proxyName }),
  updateClashRuleMode: (ruleMode: string) => invoke<void>('update_clash_rule_mode', { ruleMode }),
  reloadClashConfig: () => invoke<void>('reload_clash_config'),
  closeClashConnection: (connectionId: string) =>
    invoke<void>('close_clash_connection', { connectionId }),
  refreshClashProxyProvider: (providerName: string) =>
    invoke<void>('refresh_clash_proxy_provider', { providerName }),
  getClashConnections: () => invoke<ClashConnection[]>('get_clash_connections'),
  testClashProxyDelay: (groupName: string) =>
    invoke<number>('test_clash_proxy_delay', { groupName }),
  onCoreLog: (handler: (event: CoreLogEvent) => void) =>
    listen<CoreLogEvent>('core-log', ({ payload }) => handler(payload)),
  onAppStateChanged: (handler: (reason: string) => void) =>
    listen<string>('app-state-changed', ({ payload }) => handler(payload)),
}

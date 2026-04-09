import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

import type {
  AppConfig,
  AppStatus,
  BackgroundTaskEvent,
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

const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

// ── HTTP 客户端（浏览器模式）─────────────────────────────────────────────────

const HTTP_BASE = 'http://127.0.0.1:7393'
const WS_BASE = 'ws://127.0.0.1:7393'

async function httpGet<T>(path: string): Promise<T> {
  const res = await fetch(`${HTTP_BASE}${path}`)
  if (!res.ok) throw new Error(await res.text())
  return res.json() as Promise<T>
}

async function httpGetText(path: string): Promise<string> {
  const res = await fetch(`${HTTP_BASE}${path}`)
  if (!res.ok) throw new Error(await res.text())
  return res.text()
}

async function httpPost<T>(path: string, body?: unknown): Promise<T> {
  const res = await fetch(`${HTTP_BASE}${path}`, {
    method: 'POST',
    headers: body !== undefined ? { 'Content-Type': 'application/json' } : undefined,
    body: body !== undefined ? JSON.stringify(body) : undefined,
  })
  if (!res.ok) throw new Error(await res.text())
  return res.json() as Promise<T>
}

async function httpDelete<T>(path: string): Promise<T> {
  const res = await fetch(`${HTTP_BASE}${path}`, { method: 'DELETE' })
  if (!res.ok) throw new Error(await res.text())
  return res.json() as Promise<T>
}

// ── WebSocket 单例（浏览器模式）──────────────────────────────────────────────

type WsHandlerSet<T> = Set<(event: T) => void>

const wsHandlers = {
  coreLog: new Set<(event: CoreLogEvent) => void>(),
  stateChanged: new Set<(reason: string) => void>(),
  backgroundTask: new Set<(event: BackgroundTaskEvent) => void>(),
} as {
  coreLog: WsHandlerSet<CoreLogEvent>
  stateChanged: WsHandlerSet<string>
  backgroundTask: WsHandlerSet<BackgroundTaskEvent>
}

let wsInstance: WebSocket | null = null

function ensureWsConnected() {
  if (wsInstance && wsInstance.readyState <= WebSocket.OPEN) return

  const connect = () => {
    try {
      wsInstance = new WebSocket(`${WS_BASE}/api/events`)
    } catch (err) {
      console.warn('[WS] 连接失败，2秒后重试', err)
      setTimeout(connect, 2000)
      return
    }

    wsInstance.onmessage = (e: MessageEvent) => {
      try {
        const raw = typeof e.data === 'string' ? e.data : ''
        if (!raw) return
        const msg = JSON.parse(raw) as { event?: string; data?: unknown }
        if (!msg.event) return
        if (msg.event === 'core-log') wsHandlers.coreLog.forEach((h) => { try { h(msg.data as CoreLogEvent) } catch {} })
        else if (msg.event === 'app-state-changed') wsHandlers.stateChanged.forEach((h) => { try { h(msg.data as string) } catch {} })
        else if (msg.event === 'background-task-finished') wsHandlers.backgroundTask.forEach((h) => { try { h(msg.data as BackgroundTaskEvent) } catch {} })
      } catch {
        // ignore malformed messages
      }
    }

    wsInstance.onerror = () => {
      // onerror is always followed by onclose, no action needed
    }

    wsInstance.onclose = () => {
      wsInstance = null
      setTimeout(connect, 2000)
    }
  }

  connect()
}

// ── 统一 API 对象 ─────────────────────────────────────────────────────────────

export const desktopApi = {
  // 状态
  getStatus: isTauri
    ? () => invoke<AppStatus>('get_app_status_light')
    : () => httpGet<AppStatus>('/api/status'),

  getFullStatus: isTauri
    ? () => invoke<AppStatus>('get_app_status')
    : () => httpGet<AppStatus>('/api/status/full'),

  // 配置
  saveConfig: isTauri
    ? (config: AppConfig) => invoke<AppConfig>('save_app_config', { config })
    : (config: AppConfig) => httpPost<AppConfig>('/api/config', config),

  // 路由集
  initializeBuiltinRouting: isTauri
    ? (advancedOnly = false) => invoke<AppConfig>('initialize_builtin_routing', { advancedOnly })
    : (advancedOnly = false) => httpPost<AppConfig>('/api/routing/init', { advanced_only: advancedOnly }),

  importRoutingTemplateUrl: isTauri
    ? (url: string, advancedOnly = false) => invoke<AppConfig>('import_routing_template_url', { url, advancedOnly })
    : (url: string, advancedOnly = false) => httpPost<AppConfig>('/api/routing/template-url', { url, advanced_only: advancedOnly }),

  saveRoutingItem: isTauri
    ? (routingItem: RoutingItem) => invoke<AppConfig>('save_routing_item', { routingItem })
    : (routingItem: RoutingItem) => httpPost<AppConfig>('/api/routing/item', routingItem),

  removeRoutingItem: isTauri
    ? (routingId: string) => invoke<AppConfig>('remove_routing_item', { routingId })
    : (routingId: string) => httpDelete<AppConfig>(`/api/routing/item/${routingId}`),

  setDefaultRoutingItem: isTauri
    ? (routingId: string) => invoke<AppConfig>('set_default_routing_item', { routingId })
    : (routingId: string) => httpPost<AppConfig>(`/api/routing/item/${routingId}/default`),

  importRoutingRules: isTauri
    ? (routingId: string, raw: string, replaceExisting = false) => invoke<AppConfig>('import_routing_rules', { routingId, raw, replaceExisting })
    : (routingId: string, raw: string, replaceExisting = false) => httpPost<AppConfig>(`/api/routing/item/${routingId}/rules`, { raw, replace_existing: replaceExisting }),

  exportRoutingRules: isTauri
    ? (routingId: string, ruleIds?: string[]) => invoke<string>('export_routing_rules', { routingId, ruleIds })
    : (routingId: string) => httpGetText(`/api/routing/item/${routingId}/rules`),

  moveRoutingRule: isTauri
    ? (routingId: string, ruleId: string, direction: string) => invoke<AppConfig>('move_routing_rule', { routingId, ruleId, direction })
    : (routingId: string, ruleId: string, direction: string) => httpPost<AppConfig>(`/api/routing/item/${routingId}/rules/${ruleId}/move`, { direction }),

  // 导入
  importShareLinks: isTauri
    ? (raw: string, coreType: CoreType) => invoke<AppConfig>('import_share_links', { raw, coreType })
    : (raw: string, coreType: CoreType) => httpPost<AppConfig>('/api/import/share-links', { raw, core_type: coreType }),

  previewImport: isTauri
    ? (raw: string, coreType: CoreType) => invoke<ImportPreview>('preview_import_result', { raw, coreType })
    : (raw: string, coreType: CoreType) => httpPost<ImportPreview>('/api/import/preview', { raw, core_type: coreType }),

  importFullConfig: isTauri
    ? (raw: string) => invoke<AppConfig>('import_full_config', { raw })
    : (raw: string) => httpPost<AppConfig>('/api/import/full', { raw }),

  // 订阅
  saveSubscription: isTauri
    ? (subscription: Subscription) => invoke<AppConfig>('save_subscription', { subscription })
    : (subscription: Subscription) => httpPost<AppConfig>('/api/subscriptions', subscription),

  removeSubscription: isTauri
    ? (subscriptionId: string) => invoke<AppConfig>('remove_subscription', { subscriptionId })
    : (subscriptionId: string) => httpDelete<AppConfig>(`/api/subscriptions/${subscriptionId}`),

  refreshSubscription: isTauri
    ? (subscriptionId: string, coreType: CoreType) => invoke<AppConfig>('refresh_subscription', { subscriptionId, coreType })
    : (subscriptionId: string, coreType: CoreType) => httpPost<AppConfig>(`/api/subscriptions/${subscriptionId}/refresh`, { core_type: coreType }),

  refreshAllSubscriptions: isTauri
    ? (coreType: CoreType) => invoke<AppConfig>('refresh_all_subscriptions', { coreType })
    : (coreType: CoreType) => httpPost<AppConfig>('/api/subscriptions/refresh-all', { core_type: coreType }),

  refreshAllSubscriptionsInBackground: isTauri
    ? (coreType: CoreType) => invoke<void>('refresh_all_subscriptions_in_background', { coreType })
    : (coreType: CoreType) => httpPost<void>('/api/subscriptions/refresh-background', { core_type: coreType }),

  // 节点
  removeProfile: isTauri
    ? (profileId: string) => invoke<AppConfig>('remove_profile', { profileId })
    : (profileId: string) => httpDelete<AppConfig>(`/api/profiles/${profileId}`),

  selectProfile: isTauri
    ? (profileId: string) => invoke<AppConfig>('select_profile', { profileId })
    : (profileId: string) => httpPost<AppConfig>(`/api/profiles/${profileId}/select`),

  // 配置预览
  generatePreview: isTauri
    ? () => invoke<string>('generate_config_preview')
    : () => httpGetText('/api/preview'),

  // 核心管理
  checkCoreAssets: isTauri
    ? () => invoke<CoreAssetStatus[]>('check_core_assets')
    : () => httpGet<CoreAssetStatus[]>('/api/cores'),

  downloadCoreAsset: isTauri
    ? (coreType: CoreType) => invoke<CoreAssetStatus>('download_core_asset', { coreType })
    : (coreType: CoreType) => httpPost<CoreAssetStatus>(`/api/cores/${coreType}/download`),

  startCore: isTauri
    ? () => invoke<RunningStatus>('start_core')
    : () => httpPost<RunningStatus>('/api/core/start'),

  stopCore: isTauri
    ? () => invoke<RunningStatus>('stop_core')
    : () => httpPost<RunningStatus>('/api/core/stop'),

  restartCore: isTauri
    ? () => invoke<RunningStatus>('restart_core')
    : () => httpPost<RunningStatus>('/api/core/restart'),

  // 系统代理
  enableSystemProxy: isTauri
    ? () => invoke<AppConfig>('enable_system_proxy')
    : () => httpPost<AppConfig>('/api/proxy/enable'),

  disableSystemProxy: isTauri
    ? () => invoke<AppConfig>('disable_system_proxy')
    : () => httpPost<AppConfig>('/api/proxy/disable'),

  // 出口探测
  probeCurrentOutbound: isTauri
    ? () => invoke<ProxyProbe>('probe_current_outbound')
    : () => httpGet<ProxyProbe>('/api/probe'),

  // Clash API
  getClashProxyGroups: isTauri
    ? () => invoke<ClashProxyGroup[]>('get_clash_proxy_groups')
    : () => httpGet<ClashProxyGroup[]>('/api/clash/proxy-groups'),

  getClashProxyProviders: isTauri
    ? () => invoke<ClashProxyProvider[]>('get_clash_proxy_providers')
    : () => httpGet<ClashProxyProvider[]>('/api/clash/providers'),

  selectClashProxy: isTauri
    ? (groupName: string, proxyName: string) => invoke<void>('select_clash_proxy', { groupName, proxyName })
    : (groupName: string, proxyName: string) => httpPost<void>(`/api/clash/proxy-groups/${encodeURIComponent(groupName)}/select`, { name: proxyName }),

  updateClashRuleMode: isTauri
    ? (ruleMode: string) => invoke<void>('update_clash_rule_mode', { ruleMode })
    : (ruleMode: string) => httpPost<void>('/api/clash/rule-mode', { rule_mode: ruleMode }),

  reloadClashConfig: isTauri
    ? () => invoke<void>('reload_clash_config')
    : () => httpPost<void>('/api/clash/reload'),

  closeClashConnection: isTauri
    ? (connectionId: string) => invoke<void>('close_clash_connection', { connectionId })
    : (connectionId: string) =>
        connectionId
          ? httpDelete<void>(`/api/clash/connections/${encodeURIComponent(connectionId)}`)
          : httpDelete<void>('/api/clash/connections'),

  refreshClashProxyProvider: isTauri
    ? (providerName: string) => invoke<void>('refresh_clash_proxy_provider', { providerName })
    : (providerName: string) => httpPost<void>(`/api/clash/providers/${encodeURIComponent(providerName)}/refresh`),

  getClashConnections: isTauri
    ? () => invoke<ClashConnection[]>('get_clash_connections')
    : () => httpGet<ClashConnection[]>('/api/clash/connections'),

  testClashProxyDelay: isTauri
    ? (groupName: string) => invoke<number>('test_clash_proxy_delay', { groupName })
    : (groupName: string) =>
        httpGet<{ delay: number }>(`/api/clash/proxy-delay/${encodeURIComponent(groupName)}`).then((r) => r.delay),

  // 事件监听
  onCoreLog: isTauri
    ? (handler: (event: CoreLogEvent) => void) =>
        listen<CoreLogEvent>('core-log', ({ payload }) => handler(payload))
    : (handler: (event: CoreLogEvent) => void) => {
        ensureWsConnected()
        wsHandlers.coreLog.add(handler)
        return Promise.resolve(() => { wsHandlers.coreLog.delete(handler) })
      },

  onAppStateChanged: isTauri
    ? (handler: (reason: string) => void) =>
        listen<string>('app-state-changed', ({ payload }) => handler(payload))
    : (handler: (reason: string) => void) => {
        ensureWsConnected()
        wsHandlers.stateChanged.add(handler)
        return Promise.resolve(() => { wsHandlers.stateChanged.delete(handler) })
      },

  onBackgroundTaskFinished: isTauri
    ? (handler: (event: BackgroundTaskEvent) => void) =>
        listen<BackgroundTaskEvent>('background-task-finished', ({ payload }) => handler(payload))
    : (handler: (event: BackgroundTaskEvent) => void) => {
        ensureWsConnected()
        wsHandlers.backgroundTask.add(handler)
        return Promise.resolve(() => { wsHandlers.backgroundTask.delete(handler) })
      },
}

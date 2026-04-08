import type {
  AppConfig,
  AppStatus,
  CoreAssetStatus,
  CoreLogEvent,
  CoreType,
  Profile,
  RunningStatus,
  Subscription,
} from './types'

const STORAGE_KEY = 'v2rayn-tauri-browser-state'
const LOG_EVENT = 'v2rayn-tauri-browser-log'

function defaultProfile(): Profile {
  return {
    id: crypto.randomUUID(),
    name: '示例节点',
    core_type: 'sing_box',
    protocol: 'vless',
    server: 'example.com',
    port: 443,
    uuid: '',
    password: '',
    method: '',
    network: 'tcp',
    security: 'tls',
    tls: true,
    sni: 'example.com',
    host: '',
    path: '',
    service_name: '',
    flow: '',
    fingerprint: 'chrome',
    reality_public_key: '',
    reality_short_id: '',
    alpn: ['h2', 'http/1.1'],
    udp: true,
  }
}

function defaultConfig(): AppConfig {
  const profile = defaultProfile()
  return {
    selected_profile_id: profile.id,
    profiles: [profile],
    subscriptions: [],
    proxy: {
      http_port: 10809,
      socks_port: 10808,
      mixed_port: 10810,
      bypass_domains: ['localhost', '127.0.0.1'],
      use_system_proxy: false,
    },
    tun: {
      enabled: false,
      interface_name: 'utun233',
      mtu: 9000,
      auto_route: true,
      strict_route: false,
      stack: 'system',
    },
    dns: {
      remote_dns: '1.1.1.1',
      direct_dns: '223.5.5.5',
    },
    routing: {
      mode: 'rule',
    },
  }
}

function defaultCoreAssets(): CoreAssetStatus[] {
  return [
    {
      core_type: 'sing_box',
      installed_version: null,
      latest_version: 'browser-mock',
      download_url: null,
      executable_path: null,
    },
    {
      core_type: 'xray',
      installed_version: null,
      latest_version: 'browser-mock',
      download_url: null,
      executable_path: null,
    },
  ]
}

function defaultStatus(): AppStatus {
  return {
    paths: {
      root: 'browser://local-storage',
      bin: 'browser://bin',
      bin_configs: 'browser://binConfigs',
      gui_logs: 'browser://guiLogs',
      state_file: STORAGE_KEY,
    },
    config: defaultConfig(),
    runtime: {
      running: false,
      core_type: null,
      profile_id: null,
      executable_path: null,
      config_path: null,
    },
    core_assets: defaultCoreAssets(),
  }
}

function readState(): AppStatus {
  const raw = localStorage.getItem(STORAGE_KEY)
  if (!raw) {
    const status = defaultStatus()
    writeState(status)
    return status
  }

  try {
    const parsed = JSON.parse(raw) as AppStatus
    if (!parsed.config?.profiles?.length) {
      return defaultStatus()
    }
    return parsed
  } catch {
    return defaultStatus()
  }
}

function writeState(status: AppStatus): AppStatus {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(status))
  return status
}

function emitLog(message: string, level = 'info', source = 'browser-mock') {
  const event = new CustomEvent<CoreLogEvent>(LOG_EVENT, {
    detail: { message, level, source },
  })
  window.dispatchEvent(event)
}

function decodeBase64(input: string) {
  const normalized = input.replace(/-/g, '+').replace(/_/g, '/')
  const padding = normalized.length % 4 === 0 ? '' : '='.repeat(4 - (normalized.length % 4))
  return atob(normalized + padding)
}

function parseImportedLine(raw: string, coreType: CoreType): Profile | null {
  const line = raw.trim()
  if (!line) {
    return null
  }

  try {
    if (line.startsWith('vmess://')) {
      const payload = JSON.parse(decodeBase64(line.slice('vmess://'.length))) as Record<string, string>
      return {
        ...defaultProfile(),
        id: crypto.randomUUID(),
        name: payload.ps || 'VMess 节点',
        core_type: coreType,
        protocol: 'vmess',
        server: payload.add || '',
        port: Number(payload.port || 443),
        uuid: payload.id || '',
        network: payload.net || 'tcp',
        security: payload.tls || 'none',
        tls: payload.tls === 'tls',
        host: payload.host || '',
        path: payload.path || '',
        sni: payload.sni || '',
      }
    }

    if (line.startsWith('ss://')) {
      return {
        ...defaultProfile(),
        id: crypto.randomUUID(),
        name: 'Shadowsocks 节点',
        core_type: coreType,
        protocol: 'shadowsocks',
        security: 'none',
        tls: false,
      }
    }

    const url = new URL(line)
    if (!['vless:', 'trojan:'].includes(url.protocol)) {
      return null
    }
    return {
      ...defaultProfile(),
      id: crypto.randomUUID(),
      name: decodeURIComponent(url.hash.slice(1) || `${url.protocol.replace(':', '')} 节点`),
      core_type: coreType,
      protocol: url.protocol === 'trojan:' ? 'trojan' : 'vless',
      server: url.hostname,
      port: Number(url.port || 443),
      uuid: url.protocol === 'vless:' ? url.username : '',
      password: url.protocol === 'trojan:' ? url.username : '',
      network: url.searchParams.get('type') || 'tcp',
      security: url.searchParams.get('security') || 'tls',
      tls: ['tls', 'reality'].includes(url.searchParams.get('security') || 'tls'),
      sni: url.searchParams.get('sni') || '',
      host: url.searchParams.get('host') || '',
      path: url.searchParams.get('path') || '',
      service_name: url.searchParams.get('serviceName') || '',
      flow: url.searchParams.get('flow') || '',
      fingerprint: url.searchParams.get('fp') || 'chrome',
      reality_public_key: url.searchParams.get('pbk') || '',
      reality_short_id: url.searchParams.get('sid') || '',
      alpn: (url.searchParams.get('alpn') || '')
        .split(',')
        .map((item) => item.trim())
        .filter(Boolean),
    }
  } catch {
    return null
  }
}

function previewFromConfig(config: AppConfig) {
  const profile =
    config.profiles.find((item) => item.id === config.selected_profile_id) ?? config.profiles[0]

  return JSON.stringify(
    {
      mode: 'browser-mock',
      selectedProfile: profile,
      proxy: config.proxy,
      tun: config.tun,
      dns: config.dns,
      routing: config.routing,
    },
    null,
    2,
  )
}

export const browserApi = {
  async getStatus(): Promise<AppStatus> {
    const state = readState()
    emitLog('浏览器模式已启用，当前未连接 Tauri 后端。')
    return state
  },

  async saveConfig(config: AppConfig): Promise<AppConfig> {
    const state = readState()
    state.config = config
    writeState(state)
    emitLog('已在浏览器本地存储中保存配置。')
    return config
  },

  async importShareLinks(raw: string, coreType: CoreType): Promise<AppConfig> {
    const state = readState()
    const imported = raw
      .split('\n')
      .map((line) => parseImportedLine(line, coreType))
      .filter((profile): profile is Profile => profile !== null)

    if (!imported.length) {
      throw new Error('未识别到可导入的分享链接')
    }

    state.config.profiles.push(...imported)
    state.config.selected_profile_id = imported[0].id
    writeState(state)
    emitLog(`已导入 ${imported.length} 条分享链接。`)
    return state.config
  },

  async saveSubscription(subscription: Subscription): Promise<AppConfig> {
    const state = readState()
    const existing = state.config.subscriptions.find((item) => item.id === subscription.id)
    if (existing) {
      Object.assign(existing, subscription)
    } else {
      state.config.subscriptions.push(subscription)
    }
    writeState(state)
    emitLog(`订阅 ${subscription.name} 已保存。`)
    return state.config
  },

  async refreshSubscription(subscriptionId: string, coreType: CoreType): Promise<AppConfig> {
    const state = readState()
    const subscription = state.config.subscriptions.find((item) => item.id === subscriptionId)
    if (!subscription) {
      throw new Error('未找到订阅')
    }
    subscription.last_synced_at = new Date().toISOString()
    emitLog(`浏览器模式不会实际请求订阅，已模拟刷新 ${subscription.name}。`, 'info')
    if (!subscription.url) {
      writeState(state)
      return state.config
    }

    const imported = parseImportedLine(subscription.url, coreType)
    if (imported) {
      state.config.profiles.push(imported)
      state.config.selected_profile_id = imported.id
    }
    writeState(state)
    return state.config
  },

  async generatePreview(): Promise<string> {
    const state = readState()
    return previewFromConfig(state.config)
  },

  async checkCoreAssets(): Promise<CoreAssetStatus[]> {
    return readState().core_assets
  },

  async downloadCoreAsset(coreType: CoreType): Promise<CoreAssetStatus> {
    const state = readState()
    const asset = state.core_assets.find((item) => item.core_type === coreType)
    if (!asset) {
      throw new Error('未找到核心信息')
    }
    asset.installed_version = 'browser-mock'
    asset.latest_version = 'browser-mock'
    asset.executable_path = `browser://bin/${coreType}/${coreType === 'sing_box' ? 'sing-box' : 'xray'}`
    writeState(state)
    emitLog(`${coreType} 核心已在浏览器模式下标记为已安装。`)
    return asset
  },

  async startCore(): Promise<RunningStatus> {
    const state = readState()
    const selected =
      state.config.profiles.find((item) => item.id === state.config.selected_profile_id) ??
      state.config.profiles[0]
    state.runtime = {
      running: true,
      core_type: selected?.core_type ?? 'sing_box',
      profile_id: selected?.id ?? null,
      executable_path: `browser://bin/${selected?.core_type ?? 'sing_box'}`,
      config_path: 'browser://binConfigs/config.json',
    }
    writeState(state)
    emitLog(`已模拟启动 ${selected?.core_type ?? 'sing_box'} 核心。`)
    return state.runtime
  },

  async stopCore(): Promise<RunningStatus> {
    const state = readState()
    state.runtime = {
      running: false,
      core_type: state.runtime.core_type ?? null,
      profile_id: state.runtime.profile_id ?? null,
      executable_path: state.runtime.executable_path ?? null,
      config_path: state.runtime.config_path ?? null,
    }
    writeState(state)
    emitLog('已模拟停止核心。')
    return state.runtime
  },

  async enableSystemProxy(): Promise<AppConfig> {
    const state = readState()
    state.config.proxy.use_system_proxy = true
    writeState(state)
    emitLog('浏览器模式下已模拟开启系统代理。')
    return state.config
  },

  async disableSystemProxy(): Promise<AppConfig> {
    const state = readState()
    state.config.proxy.use_system_proxy = false
    writeState(state)
    emitLog('浏览器模式下已模拟关闭系统代理。')
    return state.config
  },

  onCoreLog(handler: (event: CoreLogEvent) => void) {
    const listener = (event: Event) => {
      handler((event as CustomEvent<CoreLogEvent>).detail)
    }
    window.addEventListener(LOG_EVENT, listener)
    return Promise.resolve(() => {
      window.removeEventListener(LOG_EVENT, listener)
    })
  },
}

export function isTauriRuntime() {
  return Boolean((window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__)
}

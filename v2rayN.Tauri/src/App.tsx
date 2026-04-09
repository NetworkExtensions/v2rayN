import type { ChangeEvent, ReactNode } from 'react'
import { useDeferredValue, useEffect, useMemo, useRef, useState } from 'react'

import { desktopApi } from './lib/api'
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
  Profile,
  RoutingItem,
  RoutingRule,
  Subscription,
} from './lib/types'

type PrimaryTabKey = 'home' | 'import' | 'exceptions' | 'advanced'
type AdvancedTabKey = 'profiles' | 'subscriptions' | 'routing' | 'settings' | 'clash' | 'logs'
type QuickMode = 'rule' | 'global' | 'direct'
type SimpleRuleKind = 'host' | 'app'
type PriorityLevel = 'high' | 'normal'

interface SimpleRuleDraft {
  target: string
  action: 'proxy' | 'direct' | 'block'
  priority: PriorityLevel
}

const primaryTabs: Array<{ key: PrimaryTabKey; label: string; description: string }> = [
  { key: 'home', label: '首页', description: '连接状态与模式切换' },
  { key: 'import', label: '添加配置', description: '统一导入入口' },
  { key: 'exceptions', label: '规则与例外', description: '网站与 App 例外' },
  { key: 'advanced', label: '高级', description: '专业模式与诊断' },
]

const advancedTabs: Array<{ key: AdvancedTabKey; label: string }> = [
  { key: 'profiles', label: '节点' },
  { key: 'subscriptions', label: '订阅' },
  { key: 'routing', label: '路由' },
  { key: 'settings', label: '设置' },
  { key: 'clash', label: 'Clash' },
  { key: 'logs', label: '日志' },
]

function App() {
  const [activeTab, setActiveTab] = useState<PrimaryTabKey>('home')
  const [advancedTab, setAdvancedTab] = useState<AdvancedTabKey>('profiles')
  const [status, setStatus] = useState<AppStatus | null>(null)
  const [config, setConfig] = useState<AppConfig | null>(null)
  const [selectedProfileId, setSelectedProfileId] = useState<string>('')
  const [logs, setLogs] = useState<CoreLogEvent[]>([])
  const [importText, setImportText] = useState('')
  const [quickSubscriptionUrl, setQuickSubscriptionUrl] = useState('')
  const [quickSubscriptionName, setQuickSubscriptionName] = useState('')
  const [preview, setPreview] = useState('')
  const [busyAction, setBusyAction] = useState<string | null>(null)
  const [message, setMessage] = useState<string>('')
  const [professionalMode, setProfessionalMode] = useState<boolean>(() => {
    if (typeof window === 'undefined') {
      return false
    }
    return window.localStorage.getItem('v2rayn-professional-mode') === 'true'
  })
  const [probeLoading, setProbeLoading] = useState(false)
  const [previewLoading, setPreviewLoading] = useState(false)
  const [previewStale, setPreviewStale] = useState(false)
  const [lastImportPreview, setLastImportPreview] = useState<ImportPreview | null>(null)
  const [clipboardSuggestion, setClipboardSuggestion] = useState<string>('')
  const [hostRuleDraft, setHostRuleDraft] = useState<SimpleRuleDraft>({ target: '', action: 'proxy', priority: 'high' })
  const [appRuleDraft, setAppRuleDraft] = useState<SimpleRuleDraft>({ target: '', action: 'direct', priority: 'normal' })
  const [clashProxyGroups, setClashProxyGroups] = useState<ClashProxyGroup[]>([])
  const [clashProxyProviders, setClashProxyProviders] = useState<ClashProxyProvider[]>([])
  const [clashConnections, setClashConnections] = useState<ClashConnection[]>([])
  const [selectedRoutingId, setSelectedRoutingId] = useState<string>('')
  const [selectedRoutingRuleId, setSelectedRoutingRuleId] = useState<string>('')
  const [routingTemplateUrlDraft, setRoutingTemplateUrlDraft] = useState('')
  const [clashConnectionFilter, setClashConnectionFilter] = useState('')
  const deferredClashConnectionFilter = useDeferredValue(clashConnectionFilter)
  const logBufferRef = useRef<CoreLogEvent[]>([])
  const appStateRefreshTimerRef = useRef<number | null>(null)
  const importFileRef = useRef<HTMLInputElement | null>(null)
  const autosavePendingRef = useRef(false)

  useEffect(() => {
    void loadStatus()
    void refreshPreview()
    const unlistenPromise = desktopApi.onCoreLog((event) => {
      logBufferRef.current.push(event)
    })
    const stateChangedPromise = desktopApi.onAppStateChanged(() => {
      if (appStateRefreshTimerRef.current !== null) {
        window.clearTimeout(appStateRefreshTimerRef.current)
      }
      appStateRefreshTimerRef.current = window.setTimeout(() => {
        appStateRefreshTimerRef.current = null
        void loadStatus()
      }, 150)
    })
    const backgroundTaskPromise = desktopApi.onBackgroundTaskFinished((event) => {
      setBusyAction((current) => (current === event.task ? null : current))
      setMessage(event.message)
      if (event.success && event.task.startsWith('subscription-refresh')) {
        setPreviewStale(true)
      }
    })

    return () => {
      if (appStateRefreshTimerRef.current !== null) {
        window.clearTimeout(appStateRefreshTimerRef.current)
      }
      void unlistenPromise.then((unlisten) => unlisten())
      void stateChangedPromise.then((unlisten) => unlisten())
      void backgroundTaskPromise.then((unlisten) => unlisten())
    }
  }, [])

  useEffect(() => {
    const timer = window.setInterval(() => {
      if (logBufferRef.current.length === 0) {
        return
      }
      const nextLogs = logBufferRef.current.splice(0, logBufferRef.current.length)
      setLogs((current) => [...current, ...nextLogs].slice(-500))
    }, 200)

    return () => window.clearInterval(timer)
  }, [])

  useEffect(() => {
    if (!message) {
      return
    }
    const timer = window.setTimeout(() => setMessage(''), 5000)
    return () => window.clearTimeout(timer)
  }, [message])

  useEffect(() => {
    if (typeof window === 'undefined') {
      return
    }
    window.localStorage.setItem('v2rayn-professional-mode', String(professionalMode))
  }, [professionalMode])

  useEffect(() => {
    if (typeof navigator === 'undefined' || !navigator.clipboard?.readText) {
      return
    }
    let cancelled = false
    void navigator.clipboard
      .readText()
      .then((text) => {
        if (cancelled || !looksLikeImportText(text)) {
          return
        }
        setClipboardSuggestion(text.trim())
      })
      .catch(() => {})
    return () => {
      cancelled = true
    }
  }, [])

  const selectedProfile = useMemo(() => {
    return config?.profiles.find((profile) => profile.id === selectedProfileId) ?? null
  }, [config, selectedProfileId])

  const selectedRouting = useMemo(() => {
    return config?.routing.items.find((item) => item.id === selectedRoutingId) ?? null
  }, [config?.routing.items, selectedRoutingId])

  const selectedRoutingRule = useMemo(() => {
    return selectedRouting?.rule_set.find((rule) => rule.id === selectedRoutingRuleId) ?? null
  }, [selectedRouting, selectedRoutingRuleId])
  const activeRoutingItem = useMemo(() => {
    return (
      config?.routing.items.find((item) => item.id === config.routing.routing_index_id) ??
      config?.routing.items.find((item) => item.is_active) ??
      config?.routing.items[0] ??
      null
    )
  }, [config])
  const [profileDraft, setProfileDraft] = useState<Profile | null>(null)
  const [routingDraft, setRoutingDraft] = useState<RoutingItem | null>(null)
  const [routingRuleDraft, setRoutingRuleDraft] = useState<RoutingRule | null>(null)
  const profileDraftDirtyRef = useRef(false)
  const routingDraftDirtyRef = useRef(false)
  const routingRuleDraftDirtyRef = useRef(false)
  const editingProfile = profileDraft ?? selectedProfile
  const editingRouting = routingDraft ?? selectedRouting
  const editingRoutingRule = routingRuleDraft ?? selectedRoutingRule
  const installedCoreTypes = useMemo(
    () =>
      new Set(
        (status?.core_assets ?? [])
          .filter((asset) => asset.installed_version || asset.executable_path)
          .map((asset) => asset.core_type),
      ),
    [status?.core_assets],
  )
  const recommendedCore = useMemo<CoreType>(() => {
    const preferred = selectedProfile?.core_type
    if (preferred && installedCoreTypes.has(preferred)) {
      return preferred
    }
    if (installedCoreTypes.has('sing_box')) {
      return 'sing_box'
    }
    if (installedCoreTypes.has('xray')) {
      return 'xray'
    }
    if (installedCoreTypes.has('mihomo')) {
      return 'mihomo'
    }
    return preferred ?? 'sing_box'
  }, [installedCoreTypes, selectedProfile?.core_type])
  const currentMode = (config?.routing.mode === 'global' || config?.routing.mode === 'direct' ? config.routing.mode : 'rule') as QuickMode
  const hostRules = useMemo(
    () =>
      (activeRoutingItem?.rule_set ?? []).filter(
        (rule) => rule.domain.length > 0 && rule.process.length === 0 && ['proxy', 'direct', 'block'].includes(rule.outbound_tag ?? ''),
      ),
    [activeRoutingItem],
  )
  const appRules = useMemo(
    () =>
      (activeRoutingItem?.rule_set ?? []).filter(
        (rule) => rule.process.length > 0 && rule.domain.length === 0 && ['proxy', 'direct', 'block'].includes(rule.outbound_tag ?? ''),
      ),
    [activeRoutingItem],
  )
  const needsCoreSetup = useMemo(() => !installedCoreTypes.has(recommendedCore), [installedCoreTypes, recommendedCore])
  const needsImportSetup = (config?.profiles.length ?? 0) === 0
  const showOnboarding = needsCoreSetup || needsImportSetup

  const sortedClashProxyGroups = useMemo(() => {
    const groups = [...clashProxyGroups]
    switch (config?.clash.proxies_sorting) {
      case 0:
        return groups.sort((a, b) => (a.last_delay_ms ?? Number.MAX_SAFE_INTEGER) - (b.last_delay_ms ?? Number.MAX_SAFE_INTEGER))
      case 1:
        return groups.sort((a, b) => a.name.localeCompare(b.name))
      default:
        return groups
    }
  }, [clashProxyGroups, config?.clash.proxies_sorting])

  const filteredClashConnections = useMemo(() => {
    const keyword = deferredClashConnectionFilter.trim().toLowerCase()
    if (!keyword) {
      return clashConnections
    }
    return clashConnections.filter((connection) =>
      [
        connection.host,
        connection.destination,
        connection.rule,
        connection.chains.join(' '),
        connection.id,
      ]
        .filter(Boolean)
        .join(' ')
        .toLowerCase()
        .includes(keyword),
    )
  }, [deferredClashConnectionFilter, clashConnections])

  useEffect(() => {
    if (activeTab === 'advanced' && advancedTab === 'clash' && status?.runtime.running && status.runtime.core_type === 'mihomo') {
      void refreshClashState()
    }
  }, [activeTab, advancedTab, status?.runtime.running, status?.runtime.core_type])

  useEffect(() => {
    const nextRoutingId =
      config?.routing.routing_index_id ??
      config?.routing.items.find((item) => item.is_active)?.id ??
      config?.routing.items[0]?.id ??
      ''
    setSelectedRoutingId((current) => (current && config?.routing.items.some((item) => item.id === current) ? current : nextRoutingId))
  }, [config?.routing.items, config?.routing.routing_index_id])

  useEffect(() => {
    const nextRuleId = selectedRouting?.rule_set[0]?.id ?? ''
    setSelectedRoutingRuleId((current) =>
      current && selectedRouting?.rule_set.some((rule) => rule.id === current) ? current : nextRuleId,
    )
  }, [selectedRouting])

  useEffect(() => {
    setRoutingTemplateUrlDraft(config?.routing.template_source_url ?? '')
  }, [config?.routing.template_source_url])

  useEffect(() => {
    profileDraftDirtyRef.current = false
    setProfileDraft(selectedProfile ? structuredClone(selectedProfile) : null)
  }, [selectedProfile])

  useEffect(() => {
    routingDraftDirtyRef.current = false
    setRoutingDraft(selectedRouting ? structuredClone(selectedRouting) : null)
  }, [selectedRouting])

  useEffect(() => {
    routingRuleDraftDirtyRef.current = false
    setRoutingRuleDraft(selectedRoutingRule ? structuredClone(selectedRoutingRule) : null)
  }, [selectedRoutingRule])

  useEffect(() => {
    if (!profileDraftDirtyRef.current || !profileDraft) {
      return
    }
    const timer = window.setTimeout(() => {
      profileDraftDirtyRef.current = false
      setConfig((current) => {
        if (!current) {
          return current
        }
        const nextConfig = structuredClone(current)
        const target = nextConfig.profiles.find((profile) => profile.id === profileDraft.id)
        if (target) {
          Object.assign(target, profileDraft)
        }
        autosavePendingRef.current = true
        return nextConfig
      })
    }, 250)
    return () => window.clearTimeout(timer)
  }, [profileDraft])

  useEffect(() => {
    if (!routingDraftDirtyRef.current || !routingDraft) {
      return
    }
    const timer = window.setTimeout(() => {
      routingDraftDirtyRef.current = false
      setConfig((current) => {
        if (!current) {
          return current
        }
        const nextConfig = structuredClone(current)
        const target = nextConfig.routing.items.find((item) => item.id === routingDraft.id)
        if (target) {
          Object.assign(target, routingDraft)
        }
        autosavePendingRef.current = true
        return nextConfig
      })
    }, 250)
    return () => window.clearTimeout(timer)
  }, [routingDraft])

  useEffect(() => {
    if (!routingRuleDraftDirtyRef.current || !routingRuleDraft || !selectedRoutingId) {
      return
    }
    const timer = window.setTimeout(() => {
      routingRuleDraftDirtyRef.current = false
      setConfig((current) => {
        if (!current) {
          return current
        }
        const nextConfig = structuredClone(current)
        const target = nextConfig.routing.items
          .find((item) => item.id === selectedRoutingId)
          ?.rule_set.find((rule) => rule.id === routingRuleDraft.id)
        if (target) {
          Object.assign(target, routingRuleDraft)
        }
        autosavePendingRef.current = true
        return nextConfig
      })
    }, 250)
    return () => window.clearTimeout(timer)
  }, [routingRuleDraft, selectedRoutingId])

  useEffect(() => {
    if (!config || !autosavePendingRef.current) {
      return
    }
    const timer = window.setTimeout(() => {
      const snapshot = buildConfigWithDrafts(config)
      autosavePendingRef.current = false
      void persistConfig(snapshot, { successMessage: '更改已自动保存', silent: true })
    }, 900)
    return () => window.clearTimeout(timer)
  }, [config])

  useEffect(() => {
    if (
      activeTab !== 'advanced' ||
      advancedTab !== 'clash' ||
      status?.runtime.core_type !== 'mihomo' ||
      !status?.runtime.running ||
      !config?.clash.proxies_auto_refresh
    ) {
      return
    }

    const intervalMinutes = Math.max(1, config.clash.proxies_auto_delay_test_interval)
    const timer = window.setInterval(() => {
      void refreshClashState(true)
    }, intervalMinutes * 60_000)

    return () => window.clearInterval(timer)
  }, [
    activeTab,
    advancedTab,
    status?.runtime.core_type,
    status?.runtime.running,
    config?.clash.proxies_auto_refresh,
    config?.clash.proxies_auto_delay_test_interval,
  ])

  useEffect(() => {
    if (
      activeTab !== 'advanced' ||
      advancedTab !== 'clash' ||
      status?.runtime.core_type !== 'mihomo' ||
      !status?.runtime.running ||
      !config?.clash.connections_auto_refresh
    ) {
      return
    }

    const intervalSteps = Math.max(1, config.clash.connections_refresh_interval)
    const timer = window.setInterval(() => {
      void refreshClashConnectionsOnly()
    }, intervalSteps * 5_000)

    return () => window.clearInterval(timer)
  }, [
    activeTab,
    advancedTab,
    status?.runtime.core_type,
    status?.runtime.running,
    config?.clash.connections_auto_refresh,
    config?.clash.connections_refresh_interval,
  ])

  useEffect(() => {
    if (
      activeTab !== 'advanced' ||
      advancedTab !== 'clash' ||
      status?.runtime.core_type !== 'mihomo' ||
      !status?.runtime.running ||
      !config?.clash.providers_auto_refresh
    ) {
      return
    }
    const intervalMinutes = Math.max(1, config.clash.providers_refresh_interval)
    const timer = window.setInterval(() => {
      void refreshClashProvidersOnly()
    }, intervalMinutes * 60_000)
    return () => window.clearInterval(timer)
  }, [
    activeTab,
    advancedTab,
    status?.runtime.core_type,
    status?.runtime.running,
    config?.clash.providers_auto_refresh,
    config?.clash.providers_refresh_interval,
  ])

  async function loadStatus() {
    try {
      const nextStatus = await desktopApi.getStatus()
      if (nextStatus && nextStatus.config) {
        setStatus(nextStatus)
        setConfig((current) => {
          if (current && autosavePendingRef.current) {
            return current
          }
          return nextStatus.config
        })
        if (!autosavePendingRef.current) {
          setSelectedProfileId(nextStatus.config.selected_profile_id ?? nextStatus.config.profiles[0]?.id ?? '')
        }
      }
    } catch (error) {
      console.error('[loadStatus] failed:', error)
      setMessage(String(error))
    }
  }

  async function refreshPreview() {
    setPreviewLoading(true)
    try {
      const generated = await desktopApi.generatePreview()
      setPreview(generated)
      setPreviewStale(false)
    } catch (error) {
      setMessage(String(error))
    } finally {
      setPreviewLoading(false)
    }
  }

  async function syncRuntimeStatus() {
    const nextStatus = await desktopApi.getStatus()
    if (nextStatus && nextStatus.config) {
      setStatus(nextStatus)
      setConfig((current) => {
        if (current && autosavePendingRef.current) {
          return current
        }
        return nextStatus.config
      })
      if (!autosavePendingRef.current) {
        setSelectedProfileId(nextStatus.config.selected_profile_id ?? nextStatus.config.profiles[0]?.id ?? '')
      }
    }
    return nextStatus
  }

  async function refreshProbe() {
    setProbeLoading(true)
    try {
      const probe = await desktopApi.probeCurrentOutbound()
      setStatus((current) => (current ? { ...current, proxy_probe: probe } : current))
      setMessage('出口信息已刷新')
    } catch (error) {
      setMessage(String(error))
    } finally {
      setProbeLoading(false)
    }
  }

  async function refreshClashState(runDelayTest = false) {
    setBusyAction('clash-refresh')
    try {
      const [groups, providers, connections] = await Promise.all([
        desktopApi.getClashProxyGroups(),
        desktopApi.getClashProxyProviders(),
        desktopApi.getClashConnections(),
      ])
      const enrichedGroups = runDelayTest
        ? await mapWithConcurrency(groups, 3, async (group) => {
            try {
              const delay = await desktopApi.testClashProxyDelay(group.name)
              return { ...group, last_delay_ms: delay }
            } catch {
              return group
            }
          })
        : groups
      setClashProxyGroups(enrichedGroups)
      setClashProxyProviders(providers)
      setClashConnections(connections)
    } catch (error) {
      setMessage(String(error))
    } finally {
      setBusyAction((current) => (current === 'clash-refresh' ? null : current))
    }
  }

  async function refreshClashConnectionsOnly() {
    try {
      const connections = await desktopApi.getClashConnections()
      setClashConnections(connections)
    } catch (error) {
      setMessage(String(error))
    }
  }

  async function refreshClashProvidersOnly() {
    try {
      const providers = await desktopApi.getClashProxyProviders()
      setClashProxyProviders(providers)
    } catch (error) {
      setMessage(String(error))
    }
  }

  async function persistConfig(
    nextConfig: AppConfig,
    options?: { successMessage?: string; silent?: boolean; trackBusy?: boolean },
  ) {
    const { successMessage, silent = false, trackBusy = false } = options ?? {}
    if (trackBusy) {
      setBusyAction('save')
    }
    try {
      const saved = await desktopApi.saveConfig(nextConfig)
      setConfig(saved)
      setSelectedProfileId(saved.selected_profile_id ?? saved.profiles[0]?.id ?? '')
      if (!silent) {
        setMessage(successMessage ?? '配置已保存')
      }
      setPreviewStale(true)
      return saved
    } catch (error) {
      setMessage(String(error))
      return null
    } finally {
      if (trackBusy) {
        setBusyAction(null)
      }
    }
  }

  function validateSelectedProfile(profile: Profile | null) {
    if (!profile) {
      return '请先选择一个节点'
    }
    if (profile.config_type === 'external') {
      if (!profile.external_config_path?.trim()) {
        return '外部配置节点必须指定配置文件路径'
      }
      return null
    }
    if (!profile.server.trim()) {
      return '节点地址不能为空'
    }
    if (!profile.port || profile.port <= 0 || profile.port > 65535) {
      return '端口范围必须在 1-65535 之间'
    }
    if (['vless', 'vmess', 'tuic'].includes(profile.protocol) && !profile.uuid?.trim()) {
      return '当前协议需要填写 UUID'
    }
    if (['trojan', 'shadowsocks', 'naive', 'anytls', 'hysteria2'].includes(profile.protocol) && !profile.password?.trim()) {
      return '当前协议需要填写密码'
    }
    if (profile.protocol === 'shadowsocks' && !profile.method?.trim()) {
      return 'Shadowsocks 需要填写加密方法'
    }
    if (profile.security === 'reality' && !profile.reality_public_key?.trim()) {
      return 'REALITY 模式需要填写 public key'
    }
    return null
  }

  function updateConfig(mutator: (draft: AppConfig) => void) {
    if (!config) {
      return
    }

    const draft: AppConfig = structuredClone(config)
    mutator(draft)
    autosavePendingRef.current = true
    setConfig(draft)
  }

  function updateProfileDraft(mutator: (profile: Profile) => void) {
    setProfileDraft((current) => {
      if (!current) {
        return current
      }
      const draft = structuredClone(current)
      mutator(draft)
      profileDraftDirtyRef.current = true
      return draft
    })
  }

  function updateRoutingDraft(mutator: (item: RoutingItem) => void) {
    setRoutingDraft((current) => {
      if (!current) {
        return current
      }
      const draft = structuredClone(current)
      mutator(draft)
      routingDraftDirtyRef.current = true
      return draft
    })
  }

  function updateRoutingRuleDraft(mutator: (rule: RoutingRule) => void) {
    setRoutingRuleDraft((current) => {
      if (!current) {
        return current
      }
      const draft = structuredClone(current)
      mutator(draft)
      routingRuleDraftDirtyRef.current = true
      return draft
    })
  }

  function buildConfigWithDrafts(baseConfig: AppConfig) {
    const draft = structuredClone(baseConfig)

    if (profileDraftDirtyRef.current && profileDraft) {
      const target = draft.profiles.find((profile) => profile.id === profileDraft.id)
      if (target) {
        Object.assign(target, profileDraft)
      }
    }

    if (routingDraftDirtyRef.current && routingDraft) {
      const target = draft.routing.items.find((item) => item.id === routingDraft.id)
      if (target) {
        Object.assign(target, routingDraft)
      }
    }

    if (routingRuleDraftDirtyRef.current && routingRuleDraft && selectedRoutingId) {
      const target = draft.routing.items
        .find((item) => item.id === selectedRoutingId)
        ?.rule_set.find((rule) => rule.id === routingRuleDraft.id)
      if (target) {
        Object.assign(target, routingRuleDraft)
      }
    }

    return draft
  }

  async function flushPendingConfigIfNeeded(successMessage?: string) {
    if (!config || !autosavePendingRef.current) {
      return config
    }
    autosavePendingRef.current = false
    return persistConfig(buildConfigWithDrafts(config), { successMessage, silent: !successMessage })
  }

  async function runWithFlushedConfig<T>(work: () => Promise<T>) {
    await flushPendingConfigIfNeeded()
    return work()
  }

  async function analyzeImportPayload(raw = importText) {
    if (!raw.trim()) {
      setMessage('请先粘贴分享链接、订阅内容或完整配置')
      return null
    }
    setBusyAction('import-preview')
    try {
      const previewResult = await desktopApi.previewImport(raw, 'sing_box')
      setLastImportPreview(previewResult)
      return previewResult
    } catch (error) {
      setMessage(String(error))
      return null
    } finally {
      setBusyAction(null)
    }
  }

  async function handleSmartImport(raw = importText) {
    if (!raw.trim()) {
      setMessage('请先粘贴分享链接、订阅内容或完整配置')
      return
    }
    if (isHttpUrl(raw)) {
      setQuickSubscriptionUrl(raw.trim())
      setMessage('这是一个网址，请使用右侧“网址获取”来添加并拉取')
      return
    }

    setBusyAction('smart-import')
    try {
      const previewResult = await desktopApi.previewImport(raw, 'sing_box')
      setLastImportPreview(previewResult)
      if (previewResult.format === 'unknown') {
        setMessage(previewResult.message ?? '无法识别导入内容')
        return
      }

      const nextConfig = previewResult.stores_as_external
        ? await desktopApi.importFullConfig(raw)
        : await desktopApi.importShareLinks(raw, recommendedCore)

      setConfig(nextConfig)
      setSelectedProfileId(nextConfig.selected_profile_id ?? nextConfig.profiles[0]?.id ?? '')
      setImportText('')
      setClipboardSuggestion('')
      setActiveTab('home')
      setMessage(
        previewResult.stores_as_external
          ? previewResult.message ?? '完整配置已导入，现在可以直接连接'
          : `已导入 ${previewResult.profile_count || nextConfig.profiles.length} 个配置，已为你推荐 ${formatCoreType(recommendedCore)} 内核`,
      )
      setPreviewStale(true)
    } catch (error) {
      setMessage(String(error))
    } finally {
      setBusyAction(null)
    }
  }

  async function handleImportFileChange(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0]
    if (!file) {
      return
    }
    try {
      const text = await file.text()
      setImportText(text)
      setMessage(`已载入本地文件：${file.name}`)
      void analyzeImportPayload(text)
    } catch (error) {
      setMessage(`读取文件失败：${error}`)
    } finally {
      event.target.value = ''
    }
  }

  async function handleReadClipboard() {
    if (typeof navigator === 'undefined' || !navigator.clipboard?.readText) {
      setMessage('当前环境不支持读取剪贴板')
      return
    }
    try {
      const text = await navigator.clipboard.readText()
      if (!looksLikeImportText(text)) {
        setMessage('剪贴板中没有发现可识别的配置内容')
        return
      }
      if (isHttpUrl(text)) {
        setQuickSubscriptionUrl(text.trim())
        setClipboardSuggestion(text.trim())
        setMessage('已读取订阅网址，请确认后点击“添加并立即拉取”')
        return
      }
      setImportText(text.trim())
      setClipboardSuggestion(text.trim())
      setMessage('已读取剪贴板内容')
      void analyzeImportPayload(text)
    } catch (error) {
      setMessage(`读取剪贴板失败：${error}`)
    }
  }

  async function handleQuickSubscriptionImport() {
    if (!quickSubscriptionUrl.trim()) {
      setMessage('请输入订阅地址')
      return
    }
    let name = quickSubscriptionName.trim()
    try {
      if (!name) {
        const parsed = new URL(quickSubscriptionUrl)
        name = parsed.hostname || `订阅 ${(config?.subscriptions.length ?? 0) + 1}`
      }
    } catch {
      setMessage('订阅地址格式不正确')
      return
    }

    setBusyAction('subscription-quick')
    try {
      const draft: Subscription = {
        id: crypto.randomUUID(),
        name,
        url: quickSubscriptionUrl.trim(),
        enabled: true,
        more_urls: [],
        user_agent: 'v2rayN-tauri',
        filter: '',
        auto_update_interval_secs: null,
        convert_core_target: recommendedCore,
        use_proxy_on_refresh: true,
        last_checked_at: null,
        last_synced_at: null,
        last_error: null,
      }
      const savedConfig = await desktopApi.saveSubscription(draft)
      const refreshedConfig = await desktopApi.refreshSubscription(draft.id, recommendedCore)
      setConfig(refreshedConfig)
      setSelectedProfileId(refreshedConfig.selected_profile_id ?? refreshedConfig.profiles[0]?.id ?? '')
      setQuickSubscriptionUrl('')
      setQuickSubscriptionName('')
      setLastImportPreview({
        format: 'share_links',
        profile_names: refreshedConfig.profiles
          .filter((profile) => profile.source_subscription_id === draft.id)
          .map((profile) => profile.name),
        profile_count: refreshedConfig.profiles.filter((profile) => profile.source_subscription_id === draft.id).length,
        stores_as_external: false,
        external_format: null,
        message: `订阅已添加，并按 ${formatCoreType(recommendedCore)} 完成转换`,
      })
      setActiveTab('home')
      setPreviewStale(true)
      setMessage(`订阅已添加并完成拉取，共导入 ${refreshedConfig.profiles.filter((profile) => profile.source_subscription_id === draft.id).length} 个节点`)
      setStatus((current) => (current ? { ...current, config: refreshedConfig } : current))
      void savedConfig
    } catch (error) {
      setMessage(String(error))
    } finally {
      setBusyAction(null)
    }
  }

  async function handleSaveSubscription(subscription: Subscription) {
    setBusyAction('subscription-save')
    try {
      const nextConfig = await desktopApi.saveSubscription(subscription)
      setConfig(nextConfig)
      setMessage('订阅已保存')
    } catch (error) {
      setMessage(String(error))
    } finally {
      setBusyAction(null)
    }
  }

  async function handleRefreshSubscription(subscriptionId: string, coreType: CoreType) {
    setBusyAction(`subscription-refresh-${subscriptionId}`)
    try {
      await flushPendingConfigIfNeeded()
      const nextConfig = await desktopApi.refreshSubscription(subscriptionId, coreType)
      setConfig(nextConfig)
      setMessage('订阅刷新完成')
      setPreviewStale(true)
    } catch (error) {
      setMessage(String(error))
    } finally {
      setBusyAction(null)
    }
  }

  async function handleCoreDownload(coreType: CoreType) {
    setBusyAction(`download-${coreType}`)
    try {
      const updatedAsset = await desktopApi.downloadCoreAsset(coreType)
      setStatus((current) =>
        current
          ? {
              ...current,
              core_assets: current.core_assets.map((asset) =>
                asset.core_type === coreType ? updatedAsset : asset,
              ),
            }
          : current,
      )
      setMessage(`${formatCoreType(coreType)} 下载完成`)
      return updatedAsset
    } catch (error) {
      setMessage(String(error))
      return null
    } finally {
      setBusyAction(null)
    }
  }

  async function handleStart() {
    if (!selectedProfile && (config?.profiles.length ?? 0) === 0) {
      setActiveTab('import')
      setMessage('先添加一个配置，再点击连接')
      return
    }
    const validationError = validateSelectedProfile(selectedProfile)
    if (validationError) {
      setMessage(validationError)
      return
    }
    await flushPendingConfigIfNeeded()
    if (selectedProfile && !installedCoreTypes.has(selectedProfile.core_type)) {
      const downloaded = await handleCoreDownload(selectedProfile.core_type)
      if (!downloaded) {
        setMessage(`未能自动准备 ${formatCoreType(selectedProfile.core_type)}，请稍后重试`)
        return
      }
    }
    setBusyAction('start')
    try {
      await desktopApi.startCore()
    } catch (error) {
      console.error('[handleStart] startCore failed:', error)
      setMessage(String(error))
      setBusyAction(null)
      return
    }

    let nextStatus: AppStatus | null = null
    try {
      nextStatus = await syncRuntimeStatus()
      setMessage('核心已启动')
      setPreviewStale(true)
    } catch (error) {
      console.error('[handleStart] syncRuntimeStatus failed:', error)
      setMessage(`核心可能已启动，但状态刷新失败: ${error}`)
    }

    try {
      await refreshProbe()
    } catch (error) {
      console.error('[handleStart] refreshProbe failed:', error)
    }

    try {
      if (nextStatus?.runtime.core_type === 'mihomo') {
        await refreshClashState()
      }
    } catch (error) {
      console.error('[handleStart] refreshClashState failed:', error)
    }

    setBusyAction(null)
  }

  async function handleStop() {
    setBusyAction('stop')
    try {
      await desktopApi.stopCore()
    } catch (error) {
      console.error('[handleStop] stopCore failed:', error)
      setMessage(String(error))
      setBusyAction(null)
      return
    }

    try { await syncRuntimeStatus() } catch (e) { console.error('[handleStop] sync failed:', e) }
    setMessage('核心已停止')
    setClashProxyGroups([])
    setClashConnections([])
    try { await refreshProbe() } catch (e) { console.error('[handleStop] probe failed:', e) }
    setBusyAction(null)
  }

  async function handleRestart() {
    const validationError = validateSelectedProfile(selectedProfile)
    if (validationError) {
      setMessage(validationError)
      return
    }
    await flushPendingConfigIfNeeded()
    if (selectedProfile && !installedCoreTypes.has(selectedProfile.core_type)) {
      const downloaded = await handleCoreDownload(selectedProfile.core_type)
      if (!downloaded) {
        setMessage(`未能自动准备 ${formatCoreType(selectedProfile.core_type)}，请稍后重试`)
        return
      }
    }
    setBusyAction('restart')
    try {
      await desktopApi.restartCore()
    } catch (error) {
      console.error('[handleRestart] restartCore failed:', error)
      setMessage(String(error))
      setBusyAction(null)
      return
    }

    let nextStatus: AppStatus | null = null
    try {
      nextStatus = await syncRuntimeStatus()
      setMessage('核心已重启')
      setPreviewStale(true)
    } catch (e) {
      console.error('[handleRestart] sync failed:', e)
      setMessage(`核心可能已重启，但状态刷新失败: ${e}`)
    }

    try { await refreshProbe() } catch (e) { console.error('[handleRestart] probe failed:', e) }

    try {
      if (nextStatus?.runtime.core_type === 'mihomo') {
        await refreshClashState()
      }
    } catch (e) { console.error('[handleRestart] clash failed:', e) }

    setBusyAction(null)
  }

  async function handleSystemProxy(enabled: boolean) {
    setBusyAction(enabled ? 'proxy-on' : 'proxy-off')
    try {
      const nextConfig = enabled
        ? await desktopApi.enableSystemProxy()
        : await desktopApi.disableSystemProxy()
      setConfig(nextConfig)
      await syncRuntimeStatus()
      setMessage(enabled ? '系统代理已开启' : '系统代理已关闭')
    } catch (error) {
      setMessage(String(error))
    } finally {
      setBusyAction(null)
    }
  }

  async function handleQuickModeChange(mode: QuickMode) {
    updateConfig((draft) => {
      draft.routing.mode = mode
      if (draft.clash.rule_mode !== 'unchanged') {
        draft.clash.rule_mode = mode
      }
    })

    if (status?.runtime.running && status.runtime.core_type === 'mihomo') {
      setBusyAction('quick-mode')
      try {
        await desktopApi.updateClashRuleMode(mode)
        await refreshClashState()
      } catch (error) {
        setMessage(`运行中的 Clash 模式切换失败：${error}`)
      } finally {
        setBusyAction(null)
      }
    }
  }

  function addSimpleRule(kind: SimpleRuleKind) {
    const draft = kind === 'host' ? hostRuleDraft : appRuleDraft
    if (!draft.target.trim()) {
      setMessage(kind === 'host' ? '请填写网站或 host' : '请填写 App 名称或进程名')
      return
    }
    if (!activeRoutingItem) {
      setMessage('当前没有可用的路由集，请先在高级中初始化路由')
      return
    }

    updateConfig((nextConfig) => {
      const routing = nextConfig.routing.items.find((item) => item.id === activeRoutingItem.id)
      if (!routing) {
        return
      }
      const rule: RoutingRule = {
        id: crypto.randomUUID(),
        rule_type: 'all',
        enabled: true,
        remarks: kind === 'host' ? `网站例外 · ${draft.target.trim()}` : `App 例外 · ${draft.target.trim()}`,
        type_name: '',
        port: '',
        network: '',
        inbound_tag: [],
        outbound_tag: draft.action,
        ip: [],
        domain: kind === 'host' ? [draft.target.trim()] : [],
        protocol: [],
        process: kind === 'app' ? [draft.target.trim()] : [],
      }
      if (draft.priority === 'high') {
        routing.rule_set.unshift(rule)
      } else {
        routing.rule_set.push(rule)
      }
      routing.rule_num = routing.rule_set.length
    })

    if (kind === 'host') {
      setHostRuleDraft({ target: '', action: 'proxy', priority: 'high' })
    } else {
      setAppRuleDraft({ target: '', action: 'direct', priority: 'normal' })
    }
  }

  function removeSimpleRule(ruleId: string) {
    if (!activeRoutingItem) {
      return
    }
    updateConfig((draft) => {
      const routing = draft.routing.items.find((item) => item.id === activeRoutingItem.id)
      if (!routing) {
        return
      }
      routing.rule_set = routing.rule_set.filter((rule) => rule.id !== ruleId)
      routing.rule_num = routing.rule_set.length
    })
  }

  async function handleClashProxySelect(groupName: string, proxyName: string) {
    setBusyAction(`clash-proxy-${groupName}`)
    try {
      await desktopApi.selectClashProxy(groupName, proxyName)
      await refreshClashState()
      setMessage(`已切换 ${groupName} -> ${proxyName}`)
    } catch (error) {
      setMessage(String(error))
    } finally {
      setBusyAction(null)
    }
  }

  async function handleCloseClashConnection(connectionId: string) {
    setBusyAction(connectionId ? `clash-close-${connectionId}` : 'clash-close-all')
    try {
      await desktopApi.closeClashConnection(connectionId)
      await refreshClashConnectionsOnly()
      setMessage(connectionId ? '连接已关闭' : '全部连接已关闭')
    } catch (error) {
      setMessage(String(error))
    } finally {
      setBusyAction(null)
    }
  }

  async function handleRefreshClashProvider(providerName: string) {
    setBusyAction(`clash-provider-${providerName}`)
    try {
      await desktopApi.refreshClashProxyProvider(providerName)
      await refreshClashProvidersOnly()
      setMessage(`已刷新 provider: ${providerName}`)
    } catch (error) {
      setMessage(String(error))
    } finally {
      setBusyAction(null)
    }
  }

  async function handleReloadClashConfig() {
    setBusyAction('clash-reload')
    try {
      await desktopApi.reloadClashConfig()
      await refreshClashState()
      setMessage('Clash 配置已热重载')
    } catch (error) {
      setMessage(String(error))
    } finally {
      setBusyAction(null)
    }
  }

  async function handleUpdateClashRuleMode(ruleMode: string) {
    setBusyAction('clash-rule-mode')
    try {
      await desktopApi.updateClashRuleMode(ruleMode)
      await refreshClashState()
      updateConfig((draft) => {
        draft.clash.rule_mode = ruleMode
      })
      setMessage(`Clash 规则模式已切换为 ${ruleMode}`)
    } catch (error) {
      setMessage(String(error))
    } finally {
      setBusyAction(null)
    }
  }

  async function handleInitializeBuiltinRouting(advancedOnly = false) {
    setBusyAction('routing-init')
    try {
      await flushPendingConfigIfNeeded()
      const nextConfig = await desktopApi.initializeBuiltinRouting(advancedOnly)
      setConfig(nextConfig)
      setMessage(advancedOnly ? '已追加内置高级路由模板' : '已初始化内置路由模板')
      setPreviewStale(true)
    } catch (error) {
      setMessage(String(error))
    } finally {
      setBusyAction(null)
    }
  }

  async function handleImportRoutingTemplateUrl() {
    if (!routingTemplateUrlDraft.trim()) {
      setMessage('请先填写路由模板 URL')
      return
    }
    setBusyAction('routing-template-url')
    try {
      await flushPendingConfigIfNeeded()
      const nextConfig = await desktopApi.importRoutingTemplateUrl(routingTemplateUrlDraft.trim(), true)
      setConfig(nextConfig)
      setMessage('路由模板已导入')
      setPreviewStale(true)
    } catch (error) {
      setMessage(String(error))
    } finally {
      setBusyAction(null)
    }
  }

  async function handleExportRoutingRules() {
    if (!selectedRouting) {
      setMessage('请先选择路由集')
      return
    }
    setBusyAction('routing-export')
    try {
      const content = await desktopApi.exportRoutingRules(selectedRouting.id)
      await navigator.clipboard.writeText(content)
      setMessage('路由规则 JSON 已复制到剪贴板')
    } catch (error) {
      setMessage(String(error))
    } finally {
      setBusyAction(null)
    }
  }

  function addRoutingItem() {
    updateConfig((draft) => {
      const item: RoutingItem = {
        id: crypto.randomUUID(),
        remarks: `路由 ${draft.routing.items.length + 1}`,
        url: '',
        rule_set: [],
        rule_num: 0,
        enabled: true,
        locked: false,
        custom_icon: '',
        custom_ruleset_path_4_singbox: '',
        domain_strategy: '',
        domain_strategy_4_singbox: '',
        sort: draft.routing.items.length + 1,
        is_active: draft.routing.items.length === 0,
      }
      draft.routing.items.push(item)
      if (item.is_active) {
        draft.routing.routing_index_id = item.id
      }
      setSelectedRoutingId(item.id)
    })
  }

  function addRoutingRule() {
    if (!selectedRouting) {
      setMessage('请先选择路由集')
      return
    }
    updateConfig((draft) => {
      const item = draft.routing.items.find((entry) => entry.id === selectedRouting.id)
      if (!item) {
        return
      }
      const rule: RoutingRule = {
        id: crypto.randomUUID(),
        rule_type: 'all',
        enabled: true,
        remarks: '新规则',
        type_name: '',
        port: '',
        network: '',
        inbound_tag: [],
        outbound_tag: 'proxy',
        ip: [],
        domain: [],
        protocol: [],
        process: [],
      }
      item.rule_set.unshift(rule)
      item.rule_num = item.rule_set.length
      setSelectedRoutingRuleId(rule.id)
    })
  }

  if (!config || !status) {
    return <div className="flex min-h-screen items-center justify-center bg-slate-950 text-slate-100">正在加载应用状态...</div>
  }

  const addProfile = () => {
    const profile: Profile = {
      id: crypto.randomUUID(),
      name: `节点 ${config.profiles.length + 1}`,
      core_type: 'sing_box',
      protocol: 'vless',
      server: '',
      port: 443,
      uuid: '',
      password: '',
      method: '',
      network: 'tcp',
      security: 'tls',
      tls: true,
      sni: '',
      host: '',
      path: '',
      service_name: '',
      flow: '',
      fingerprint: 'chrome',
      reality_public_key: '',
      reality_short_id: '',
      alpn: [],
      udp: true,
      mux_override: 'follow_global',
      source_subscription_id: null,
      config_type: 'native',
      external_config_format: null,
      external_config_path: null,
    }
    updateConfig((draft) => {
      draft.profiles.push(profile)
      draft.selected_profile_id = profile.id
    })
    setSelectedProfileId(profile.id)
  }

  const addSubscription = () => {
    const subscription: Subscription = {
      id: crypto.randomUUID(),
      name: `订阅 ${config.subscriptions.length + 1}`,
      url: '',
      enabled: true,
      more_urls: [],
      user_agent: 'v2rayN-tauri',
      filter: '',
      auto_update_interval_secs: null,
      convert_core_target: null,
      use_proxy_on_refresh: true,
      last_checked_at: null,
      last_synced_at: null,
      last_error: null,
    }
    updateConfig((draft) => {
      draft.subscriptions.push(subscription)
    })
  }

  const currentTitle =
    activeTab === 'advanced'
      ? `高级 · ${advancedTabs.find((tab) => tab.key === advancedTab)?.label ?? '专业模式'}`
      : primaryTabs.find((tab) => tab.key === activeTab)?.label ?? '首页'
  const currentDescription =
    activeTab === 'advanced'
      ? professionalMode
        ? '保留全部极客能力，但默认不打扰普通用户。'
        : '需要时再开启专业模式，日常使用优先走普通路径。'
      : primaryTabs.find((tab) => tab.key === activeTab)?.description ?? ''

  return (
    <div className="min-h-screen bg-slate-950 text-slate-100">
      <input
        ref={importFileRef}
        type="file"
        className="hidden"
        accept=".txt,.json,.yaml,.yml,.conf,.cfg"
        onChange={(event) => void handleImportFileChange(event)}
      />
      <div className="mx-auto flex min-h-screen max-w-[1600px]">
        <aside className="w-60 border-r border-slate-800 bg-slate-900/80 p-5">
          <div className="mb-8">
            <p className="text-xs uppercase tracking-[0.3em] text-violet-300">v2rayN</p>
            <h1 className="mt-3 text-2xl font-semibold">轻量交互版</h1>
            <p className="mt-2 text-sm text-slate-400">先让普通用户顺利连上网，再把高级能力收进专业模式。</p>
          </div>
          <nav className="space-y-2">
            {primaryTabs.map((tab) => (
              <button
                key={tab.key}
                className={`w-full rounded-xl px-3 py-2 text-left text-sm transition ${
                  activeTab === tab.key
                    ? 'bg-violet-500/20 text-violet-200'
                    : 'text-slate-300 hover:bg-slate-800 hover:text-white'
                }`}
                onClick={() => setActiveTab(tab.key)}
              >
                <div className="font-medium">{tab.label}</div>
                <div className="mt-1 text-xs text-slate-500">{tab.description}</div>
              </button>
            ))}
          </nav>
          <div className="mt-6 rounded-2xl border border-slate-800 bg-slate-950/70 p-4">
            <div className="flex items-center justify-between gap-3">
              <div>
                <p className="text-sm font-medium text-slate-100">专业模式</p>
                <p className="mt-1 text-xs text-slate-400">显示协议、路由、Clash 与诊断能力</p>
              </div>
              <button
                className={`rounded-full px-3 py-1 text-xs font-medium ${professionalMode ? 'bg-violet-500 text-white' : 'bg-slate-800 text-slate-300'}`}
                onClick={() => {
                  setProfessionalMode((current) => !current)
                  setActiveTab('advanced')
                }}
              >
                {professionalMode ? '已开启' : '未开启'}
              </button>
            </div>
          </div>
          <div className="mt-8 rounded-2xl border border-slate-800 bg-slate-950/70 p-4 text-xs text-slate-400">
            <p>数据目录</p>
            <p className="mt-2 break-all font-mono text-[11px] text-slate-300">{status.paths.root}</p>
          </div>
        </aside>

        <main className="flex-1 overflow-auto p-6">
          <header className="mb-6 flex flex-wrap items-center justify-between gap-4 rounded-3xl border border-slate-800 bg-slate-900/70 px-5 py-4">
            <div>
              <h2 className="text-xl font-semibold">{currentTitle}</h2>
              <p className="mt-1 text-sm text-slate-400">
                {currentDescription} 当前配置：{selectedProfile?.name ?? '未选择'} · 运行状态：
                <span className={status.runtime.running ? 'text-emerald-300' : 'text-slate-300'}>
                  {status.runtime.running ? ' 已启动' : ' 未启动'}
                </span>
              </p>
            </div>
            <div className="flex flex-wrap items-center gap-3">
              <button className="rounded-xl bg-violet-500 px-4 py-2 text-sm font-medium text-white disabled:cursor-not-allowed disabled:bg-slate-700" onClick={handleStart} disabled={busyAction !== null}>
                {busyAction === 'start' ? '连接中...' : status.runtime.running ? '重新连接' : '立即连接'}
              </button>
              <button className="rounded-xl border border-slate-700 px-4 py-2 text-sm text-slate-200 disabled:cursor-not-allowed disabled:text-slate-500" onClick={handleStop} disabled={busyAction !== null}>
                {busyAction === 'stop' ? '停止中...' : '停止'}
              </button>
              <button className="rounded-xl border border-slate-700 px-4 py-2 text-sm text-slate-200 disabled:cursor-not-allowed disabled:text-slate-500" onClick={handleRestart} disabled={busyAction !== null}>
                {busyAction === 'restart' ? '重启中...' : '重启核心'}
              </button>
            </div>
          </header>

          {message ? (
            <div className="mb-5 rounded-2xl border border-violet-500/30 bg-violet-500/10 px-4 py-3 text-sm text-violet-100">
              {message}
            </div>
          ) : null}

          {activeTab === 'home' ? (
            <div className="grid gap-5 xl:grid-cols-[1.15fr_0.85fr]">
              <div className="space-y-5">
                {showOnboarding ? (
                  <SectionCard title="首次使用向导">
                    <div className="grid gap-4 md:grid-cols-3">
                      <QuickStepCard
                        index={1}
                        title="准备内核"
                        done={!needsCoreSetup}
                        description={needsCoreSetup ? `推荐先安装 ${formatCoreType(recommendedCore)}，这样导入后可以直接连接。` : '已检测到可用内核。'}
                        action={
                          needsCoreSetup ? (
                            <ActionButton busy={busyAction === `download-${recommendedCore}`} onClick={() => void handleCoreDownload(recommendedCore)}>
                              安装推荐内核
                            </ActionButton>
                          ) : undefined
                        }
                      />
                      <QuickStepCard
                        index={2}
                        title="添加配置"
                        done={!needsImportSetup}
                        description={needsImportSetup ? '支持本地文件、订阅地址和分享链接，系统会自动识别。' : `当前已有 ${config.profiles.length} 个配置可用。`}
                        action={
                          needsImportSetup ? (
                            <button className="rounded-xl border border-slate-700 px-3 py-2 text-sm text-slate-200" onClick={() => setActiveTab('import')}>
                              去添加配置
                            </button>
                          ) : undefined
                        }
                      />
                      <QuickStepCard
                        index={3}
                        title="一键连接"
                        done={status.runtime.running}
                        description={status.runtime.running ? '当前代理已运行，可以开始使用。' : '准备好后点击右上角“立即连接”即可。'}
                      />
                    </div>
                  </SectionCard>
                ) : null}

                <SectionCard
                  title="当前连接"
                  action={
                    !selectedProfile ? (
                      <button className="rounded-xl border border-slate-700 px-3 py-2 text-sm text-slate-200" onClick={() => setActiveTab('import')}>
                        添加配置
                      </button>
                    ) : null
                  }
                >
                  <div className="grid gap-4 md:grid-cols-[1fr_auto]">
                    <div className="space-y-4">
                      <Field label="当前使用的配置">
                        <select
                          value={selectedProfileId}
                          onChange={(event) => {
                            const nextId = event.target.value
                            setSelectedProfileId(nextId)
                            updateConfig((draft) => {
                              draft.selected_profile_id = nextId
                            })
                          }}
                        >
                          <option value="">请选择一个配置</option>
                          {config.profiles.map((profile) => (
                            <option key={profile.id} value={profile.id}>
                              {profile.name} · {profile.config_type === 'external' ? '外部配置' : profile.protocol}
                            </option>
                          ))}
                        </select>
                      </Field>
                      {selectedProfile ? (
                        <div className="grid gap-3 rounded-2xl border border-slate-800 bg-slate-950/70 p-4 md:grid-cols-3">
                          <KeyValue label="连接目标" value={selectedProfile.config_type === 'external' ? selectedProfile.external_config_path ?? '外部配置' : `${selectedProfile.server}:${selectedProfile.port}`} mono={selectedProfile.config_type === 'external'} />
                          <KeyValue label="推荐内核" value={formatCoreType(selectedProfile.core_type)} />
                          <KeyValue label="配置类型" value={selectedProfile.config_type === 'external' ? '完整外部配置' : '标准节点'} />
                        </div>
                      ) : (
                        <EmptyState
                          title="还没有可连接的配置"
                          description="普通用户只需要去“添加配置”，粘贴链接或导入文件即可。"
                          action={<button className="rounded-xl border border-slate-700 px-3 py-2 text-sm text-slate-200" onClick={() => setActiveTab('import')}>去添加配置</button>}
                        />
                      )}
                    </div>
                    <div className="grid gap-3 self-start">
                      <StatusPill label={status.runtime.running ? '已连接' : '未连接'} tone={status.runtime.running ? 'success' : 'muted'} />
                      <StatusPill label={config.proxy.use_system_proxy ? '系统代理已开启' : '系统代理未开启'} tone={config.proxy.use_system_proxy ? 'success' : 'muted'} />
                      <StatusPill label={config.tun.enabled ? 'TUN 已开启' : 'TUN 未开启'} tone={config.tun.enabled ? 'success' : 'muted'} />
                    </div>
                  </div>
                </SectionCard>

                <SectionCard title="上网模式">
                  <p className="mb-4 text-sm text-slate-400">普通用户日常只需要在这里切换，不必再去设置或 Clash 页面理解底层术语。</p>
                  <div className="grid gap-3 md:grid-cols-3">
                    <ModeOptionCard
                      active={currentMode === 'rule'}
                      title="智能分流"
                      description="推荐默认模式。常见网站自动判断，需要例外时去下一页单独设置。"
                      onClick={() => void handleQuickModeChange('rule')}
                    />
                    <ModeOptionCard
                      active={currentMode === 'global'}
                      title="全局代理"
                      description="全部流量优先走代理，适合临时排障或懒得区分场景。"
                      onClick={() => void handleQuickModeChange('global')}
                    />
                    <ModeOptionCard
                      active={currentMode === 'direct'}
                      title="直接连接"
                      description="全部流量直连，适合临时关闭代理或排查本地网络问题。"
                      onClick={() => void handleQuickModeChange('direct')}
                    />
                  </div>
                  <div className="mt-4 flex flex-wrap gap-3">
                    <button
                      className="rounded-xl border border-slate-700 px-3 py-2 text-sm text-slate-200 disabled:cursor-not-allowed disabled:opacity-50"
                      disabled={busyAction === 'proxy-on' || busyAction === 'proxy-off'}
                      onClick={() => void handleSystemProxy(!config.proxy.use_system_proxy)}
                    >
                      {config.proxy.use_system_proxy ? '关闭系统代理' : '开启系统代理'}
                    </button>
                    <button
                      className="rounded-xl border border-slate-700 px-3 py-2 text-sm text-slate-200"
                      onClick={() =>
                        updateConfig((draft) => {
                          draft.tun.enabled = !draft.tun.enabled
                        })
                      }
                    >
                      {config.tun.enabled ? '关闭 TUN' : '开启 TUN'}
                    </button>
                  </div>
                </SectionCard>

                <SectionCard
                  title="网站与 App 例外"
                  action={
                    <button className="rounded-xl border border-slate-700 px-3 py-2 text-sm text-slate-200" onClick={() => setActiveTab('exceptions')}>
                      去管理例外
                    </button>
                  }
                >
                  <div className="grid gap-4 md:grid-cols-2">
                    <SummaryStat title="网站例外" value={`${hostRules.length} 条`} description={hostRules[0]?.domain[0] ?? '尚未添加'} />
                    <SummaryStat title="App 例外" value={`${appRules.length} 条`} description={appRules[0]?.process[0] ?? '尚未添加'} />
                  </div>
                </SectionCard>
              </div>

              <div className="space-y-5">
                <SectionCard title="核心准备">
                  <div className="grid gap-4">
                    {status.core_assets.map((asset) => (
                      <CoreCard
                        key={asset.core_type}
                        asset={asset}
                        busy={busyAction === `download-${asset.core_type}`}
                        recommended={asset.core_type === recommendedCore}
                        onDownload={() => void handleCoreDownload(asset.core_type)}
                      />
                    ))}
                  </div>
                </SectionCard>

                <SectionCard
                  title="出口探测"
                  action={
                    <ActionButton busy={probeLoading} onClick={() => void refreshProbe()}>
                      刷新出口
                    </ActionButton>
                  }
                >
                  <div className="space-y-3 text-sm text-slate-300">
                    <KeyValue label="出口 IP" value={status.proxy_probe?.outbound_ip ?? '-'} />
                    <KeyValue label="国家" value={status.proxy_probe?.country ?? '-'} />
                    <KeyValue label="城市" value={status.proxy_probe?.city ?? '-'} />
                    <KeyValue label="运营商" value={status.proxy_probe?.isp ?? '-'} />
                  </div>
                </SectionCard>

                <SectionCard title="运行时概览">
                  <div className="space-y-3 text-sm text-slate-300">
                    <KeyValue label="运行内核" value={status.runtime.core_type ? formatCoreType(status.runtime.core_type) : '未启动'} />
                    <KeyValue label="配置文件" value={status.runtime.config_path ?? '-'} mono />
                    <KeyValue label="执行文件" value={status.runtime.executable_path ?? '-'} mono />
                    <KeyValue label="主进程 PID" value={status.runtime.pid ? String(status.runtime.pid) : '-'} />
                    <KeyValue label="提权启动" value={status.runtime.elevated ? '是' : '否'} />
                  </div>
                </SectionCard>
              </div>
            </div>
          ) : null}

          {activeTab === 'import' ? (
            <div className="grid gap-5 xl:grid-cols-[1.15fr_0.85fr]">
              <SectionCard title="统一导入中心">
                <p className="mb-4 text-sm text-slate-400">把本地文件、订阅内容、分享链接和完整配置统一收敛到一个入口，系统会尽量自动识别，不再让用户自己理解 sing-box / Xray 差异。</p>
                <textarea
                  value={importText}
                  onChange={(event) => setImportText(event.target.value)}
                  className="h-72 w-full rounded-2xl border border-slate-700 bg-slate-950 px-4 py-3 text-sm outline-none"
                  placeholder="粘贴 vless://、vmess://、trojan://、订阅内容、JSON 或 Clash YAML"
                />
                <div className="mt-4 flex flex-wrap gap-3">
                  <ActionButton busy={busyAction === 'smart-import'} onClick={() => void handleSmartImport()}>
                    自动导入并推荐内核
                  </ActionButton>
                  <ActionButton busy={busyAction === 'import-preview'} onClick={() => void analyzeImportPayload()}>
                    识别内容
                  </ActionButton>
                  <button className="rounded-xl border border-slate-700 px-4 py-2 text-sm text-slate-200" onClick={() => importFileRef.current?.click()}>
                    从本地文件添加
                  </button>
                  <button className="rounded-xl border border-slate-700 px-4 py-2 text-sm text-slate-200" onClick={() => void handleReadClipboard()}>
                    读取剪贴板
                  </button>
                </div>
                {clipboardSuggestion ? (
                  <div className="mt-4 rounded-2xl border border-emerald-500/30 bg-emerald-500/10 p-4 text-sm text-emerald-100">
                    <p>检测到剪贴板里可能有可导入的配置内容。</p>
                    <div className="mt-3 flex flex-wrap gap-3">
                      <button className="rounded-xl border border-emerald-400/40 px-3 py-2 text-sm text-emerald-100" onClick={() => {
                        if (isHttpUrl(clipboardSuggestion)) {
                          setQuickSubscriptionUrl(clipboardSuggestion)
                          setActiveTab('import')
                          setMessage('检测到网址，已填入“网址获取”区域')
                          return
                        }
                        setImportText(clipboardSuggestion)
                        void handleSmartImport(clipboardSuggestion)
                      }}>
                        直接导入剪贴板
                      </button>
                      <button className="rounded-xl border border-slate-700 px-3 py-2 text-sm text-slate-200" onClick={() => setImportText(clipboardSuggestion)}>
                        仅填入编辑框
                      </button>
                    </div>
                  </div>
                ) : null}
                {lastImportPreview ? (
                  <div className="mt-4 rounded-2xl border border-slate-800 bg-slate-950/70 p-4 text-sm text-slate-300">
                    <div className="grid gap-3 md:grid-cols-3">
                      <KeyValue label="识别结果" value={formatImportFormat(lastImportPreview.format)} />
                      <KeyValue label="配置数量" value={String(lastImportPreview.profile_count)} />
                      <KeyValue label="导入方式" value={lastImportPreview.stores_as_external ? '完整外部配置' : `标准节点 · ${formatCoreType(recommendedCore)}`} />
                    </div>
                    <p className="mt-3 text-slate-300">{lastImportPreview.message ?? '已完成识别。'}</p>
                    <p className="mt-2 text-xs text-slate-500">{lastImportPreview.profile_names.join('、') || '当前没有可预览的节点名称。'}</p>
                  </div>
                ) : null}
              </SectionCard>

              <div className="space-y-5">
                <SectionCard title="网址获取">
                  <div className="grid gap-4">
                    <Field label="订阅地址">
                      <input value={quickSubscriptionUrl} onChange={(event) => setQuickSubscriptionUrl(event.target.value)} placeholder="https://example.com/sub.txt" />
                    </Field>
                    <Field label="显示名称（可选）">
                      <input value={quickSubscriptionName} onChange={(event) => setQuickSubscriptionName(event.target.value)} placeholder="留空则自动使用域名" />
                    </Field>
                    <ActionButton busy={busyAction === 'subscription-quick'} onClick={() => void handleQuickSubscriptionImport()}>
                      添加并立即拉取
                    </ActionButton>
                  </div>
                </SectionCard>

                <SectionCard title="给普通用户的建议">
                  <div className="space-y-3 text-sm text-slate-300">
                    <p>推荐优先级：订阅地址 &gt; 分享链接 &gt; 完整配置文件。</p>
                    <p>如果你只拿到一串 `vless://...` 或 `vmess://...`，直接粘贴到左侧然后点“自动导入”即可。</p>
                    <p>如果你拿到的是整份 JSON / YAML，也不需要手动区分格式，系统会自动判断并按外部配置导入。</p>
                  </div>
                </SectionCard>

                <SectionCard title="导入后会发生什么">
                  <div className="space-y-3 text-sm text-slate-300">
                    <p>1. 自动识别输入属于分享链接、订阅内容还是完整配置。</p>
                    <p>2. 自动为普通用户推荐最合适的内核，优先使用 `sing-box`。</p>
                    <p>3. 导入成功后直接回到首页，点击“立即连接”就能开始使用。</p>
                  </div>
                </SectionCard>
              </div>
            </div>
          ) : null}

          {activeTab === 'exceptions' ? (
            <div className="space-y-5">
              {currentMode !== 'rule' ? (
                <div className="rounded-2xl border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-sm text-amber-100">
                  当前模式是“{getModeLabel(currentMode)}”。网站与 App 例外主要在“智能分流”下生效，如需立即生效建议先切回“智能分流”。
                </div>
              ) : null}
              <SectionCard
                title="当前正在编辑的规则集"
                action={
                  <button className="rounded-xl border border-slate-700 px-3 py-2 text-sm text-slate-200" onClick={() => {
                    setActiveTab('advanced')
                    setAdvancedTab('routing')
                    setProfessionalMode(true)
                  }}>
                    打开高级路由
                  </button>
                }
              >
                <div className="grid gap-4 md:grid-cols-3">
                  <SummaryStat title="默认路由集" value={activeRoutingItem?.remarks ?? '未初始化'} description={activeRoutingItem ? `规则数 ${activeRoutingItem.rule_num}` : '可在高级中初始化'} />
                  <SummaryStat title="网站例外" value={`${hostRules.length} 条`} description={hostRules[0]?.domain[0] ?? '暂时为空'} />
                  <SummaryStat title="App 例外" value={`${appRules.length} 条`} description={appRules[0]?.process[0] ?? '暂时为空'} />
                </div>
              </SectionCard>

              <div className="grid gap-5 xl:grid-cols-2">
                <SectionCard title="网站例外">
                  <div className="grid gap-4 md:grid-cols-[1fr_180px_160px_auto]">
                    <Field label="Host / 域名">
                      <input value={hostRuleDraft.target} onChange={(event) => setHostRuleDraft((current) => ({ ...current, target: event.target.value }))} placeholder="example.com" />
                    </Field>
                    <Field label="动作">
                      <select value={hostRuleDraft.action} onChange={(event) => setHostRuleDraft((current) => ({ ...current, action: event.target.value as SimpleRuleDraft['action'] }))}>
                        <option value="proxy">走代理</option>
                        <option value="direct">直接连接</option>
                        <option value="block">阻止</option>
                      </select>
                    </Field>
                    <Field label="优先级">
                      <select value={hostRuleDraft.priority} onChange={(event) => setHostRuleDraft((current) => ({ ...current, priority: event.target.value as PriorityLevel }))}>
                        <option value="high">高</option>
                        <option value="normal">普通</option>
                      </select>
                    </Field>
                    <div className="flex items-end">
                      <ActionButton onClick={() => addSimpleRule('host')}>新增规则</ActionButton>
                    </div>
                  </div>
                  <div className="mt-4 space-y-3">
                    {hostRules.length === 0 ? <EmptyState title="还没有网站例外" description="例如：让 `google.com` 走代理，或让公司内网域名直接连接。" /> : null}
                    {hostRules.map((rule) => (
                      <SimpleRuleCard key={rule.id} title={rule.domain.join(', ')} action={rule.outbound_tag ?? 'proxy'} remarks={rule.remarks ?? ''} onRemove={() => removeSimpleRule(rule.id)} />
                    ))}
                  </div>
                </SectionCard>

                <SectionCard title="App 例外">
                  <div className="grid gap-4 md:grid-cols-[1fr_180px_160px_auto]">
                    <Field label="App / 进程名">
                      <input value={appRuleDraft.target} onChange={(event) => setAppRuleDraft((current) => ({ ...current, target: event.target.value }))} placeholder="Telegram.exe / WeChat" />
                    </Field>
                    <Field label="动作">
                      <select value={appRuleDraft.action} onChange={(event) => setAppRuleDraft((current) => ({ ...current, action: event.target.value as SimpleRuleDraft['action'] }))}>
                        <option value="proxy">走代理</option>
                        <option value="direct">直接连接</option>
                        <option value="block">阻止</option>
                      </select>
                    </Field>
                    <Field label="优先级">
                      <select value={appRuleDraft.priority} onChange={(event) => setAppRuleDraft((current) => ({ ...current, priority: event.target.value as PriorityLevel }))}>
                        <option value="high">高</option>
                        <option value="normal">普通</option>
                      </select>
                    </Field>
                    <div className="flex items-end">
                      <ActionButton onClick={() => addSimpleRule('app')}>新增规则</ActionButton>
                    </div>
                  </div>
                  <div className="mt-4 space-y-3">
                    {appRules.length === 0 ? <EmptyState title="还没有 App 例外" description="例如：让 `Telegram` 全部走代理，或者让公司办公软件直接连接。" /> : null}
                    {appRules.map((rule) => (
                      <SimpleRuleCard key={rule.id} title={rule.process.join(', ')} action={rule.outbound_tag ?? 'proxy'} remarks={rule.remarks ?? ''} onRemove={() => removeSimpleRule(rule.id)} />
                    ))}
                  </div>
                </SectionCard>
              </div>
            </div>
          ) : null}

          {activeTab === 'advanced' ? (
            <div className="mb-5 space-y-5">
              <SectionCard title="高级能力入口">
                {!professionalMode ? (
                  <div className="space-y-4 text-sm text-slate-300">
                    <p>普通模式下，协议细节、路由模板、Clash 调试和日志默认折叠，避免普通用户被复杂概念打断。</p>
                    <p>需要排障、手工改协议参数或维护路由模板时，再开启专业模式即可。</p>
                    <button className="rounded-xl border border-slate-700 px-3 py-2 text-sm text-slate-200" onClick={() => setProfessionalMode(true)}>
                      开启专业模式
                    </button>
                  </div>
                ) : (
                  <div className="flex flex-wrap gap-2">
                    {advancedTabs.map((tab) => (
                      <button
                        key={tab.key}
                        className={`rounded-xl px-3 py-2 text-sm ${advancedTab === tab.key ? 'bg-violet-500/20 text-violet-100' : 'border border-slate-700 text-slate-300'}`}
                        onClick={() => setAdvancedTab(tab.key)}
                      >
                        {tab.label}
                      </button>
                    ))}
                  </div>
                )}
              </SectionCard>

              {!professionalMode ? (
                <div className="grid gap-5 xl:grid-cols-[1fr_1fr]">
                  <SectionCard title="核心安装状态">
                    <div className="grid gap-4">
                      {status.core_assets.map((asset) => (
                        <CoreCard
                          key={asset.core_type}
                          asset={asset}
                          busy={busyAction === `download-${asset.core_type}`}
                          recommended={asset.core_type === recommendedCore}
                          onDownload={() => void handleCoreDownload(asset.core_type)}
                        />
                      ))}
                    </div>
                  </SectionCard>
                  <SectionCard
                    title="配置预览"
                    action={
                      <ActionButton busy={previewLoading} onClick={() => void refreshPreview()}>
                        刷新预览
                      </ActionButton>
                    }
                  >
                    {previewStale ? (
                      <div className="mb-3 rounded-2xl border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-sm text-amber-100">
                        当前预览可能已过期，等待自动保存完成后再刷新会更准确。
                      </div>
                    ) : null}
                    <pre className="max-h-[34rem] overflow-auto rounded-2xl bg-slate-950 p-4 text-xs text-slate-200">
                      {preview}
                    </pre>
                  </SectionCard>
                </div>
              ) : null}
            </div>
          ) : null}

          {activeTab === 'advanced' && professionalMode && advancedTab === 'profiles' ? (
            <div className="grid gap-5 lg:grid-cols-[360px_1fr]">
              <SectionCard title="节点列表" action={<button className="rounded-xl border border-slate-700 px-3 py-2 text-sm" onClick={addProfile}>新增节点</button>}>
                <div className="space-y-2">
                  {config.profiles.map((profile) => (
                    <button
                      key={profile.id}
                      className={`w-full rounded-2xl border px-4 py-3 text-left ${
                        selectedProfileId === profile.id
                          ? 'border-violet-500/50 bg-violet-500/10'
                          : 'border-slate-800 bg-slate-900/60'
                      }`}
                      onClick={() => {
                        setSelectedProfileId(profile.id)
                        updateConfig((draft) => {
                          draft.selected_profile_id = profile.id
                        })
                      }}
                    >
                      <p className="font-medium text-slate-100">{profile.name}</p>
                      <p className="mt-1 text-xs uppercase tracking-wide text-slate-400">
                        {profile.config_type === 'external' ? 'external' : profile.protocol} · {profile.core_type}
                      </p>
                      <p className="mt-2 text-sm text-slate-400">
                        {profile.config_type === 'external'
                          ? (profile.external_config_path ?? '未设置外部配置')
                          : `${profile.server}:${profile.port}`}
                      </p>
                      <button
                        className="mt-3 rounded-lg border border-slate-700 px-2 py-1 text-xs text-slate-300 hover:border-rose-400 hover:text-rose-200"
                        onClick={(event) => {
                          event.stopPropagation()
                          void runWithFlushedConfig(() => desktopApi.removeProfile(profile.id))
                            .then((nextConfig) => {
                              setConfig(nextConfig)
                              setSelectedProfileId(nextConfig.selected_profile_id ?? nextConfig.profiles[0]?.id ?? '')
                            })
                            .catch((error) => setMessage(String(error)))
                        }}
                      >
                        删除
                      </button>
                    </button>
                  ))}
                </div>
              </SectionCard>

              <SectionCard title="节点编辑">
                {editingProfile ? (
                  <div className="grid gap-4 md:grid-cols-2">
                    <Field label="节点名称">
                      <input value={editingProfile.name} onChange={(event) => updateProfileDraft((profile) => { profile.name = event.target.value })} />
                    </Field>
                    <Field label="核心">
                      <select value={editingProfile.core_type} onChange={(event) => updateProfileDraft((profile) => { profile.core_type = event.target.value as CoreType })}>
                        <option value="sing_box">sing-box</option>
                        <option value="xray">Xray</option>
                        <option value="mihomo">mihomo</option>
                      </select>
                    </Field>
                    <Field label="配置类型">
                      <select
                        value={editingProfile.config_type}
                        onChange={(event) =>
                          updateProfileDraft((profile) => {
                            profile.config_type = event.target.value as Profile['config_type']
                          })
                        }
                      >
                        <option value="native">native</option>
                        <option value="external">external</option>
                      </select>
                    </Field>
                    <Field label="Mux 覆盖">
                      <select
                        value={editingProfile.mux_override}
                        onChange={(event) =>
                          updateProfileDraft((profile) => {
                            profile.mux_override = event.target.value as Profile['mux_override']
                          })
                        }
                      >
                        <option value="follow_global">跟随全局</option>
                        <option value="force_enable">强制开启</option>
                        <option value="force_disable">强制关闭</option>
                      </select>
                    </Field>
                    {editingProfile.config_type === 'external' ? (
                      <>
                        <Field label="外部配置格式">
                          <select
                            value={editingProfile.external_config_format ?? 'clash'}
                            onChange={(event) =>
                              updateProfileDraft((profile) => {
                                profile.external_config_format = event.target.value as NonNullable<
                                  Profile['external_config_format']
                                >
                              })
                            }
                          >
                            <option value="sing_box">sing-box JSON</option>
                            <option value="xray">Xray JSON</option>
                            <option value="clash">Clash YAML</option>
                          </select>
                        </Field>
                        <Field label="外部配置路径">
                          <input
                            value={editingProfile.external_config_path ?? ''}
                            onChange={(event) =>
                              updateProfileDraft((profile) => {
                                profile.external_config_path = event.target.value
                              })
                            }
                          />
                        </Field>
                      </>
                    ) : (
                      <>
                    <Field label="协议">
                      <select value={editingProfile.protocol} onChange={(event) => updateProfileDraft((profile) => { profile.protocol = event.target.value as Profile['protocol'] })}>
                        <option value="vless">VLESS</option>
                        <option value="vmess">VMess</option>
                        <option value="trojan">Trojan</option>
                        <option value="shadowsocks">Shadowsocks</option>
                        <option value="hysteria2">Hysteria2</option>
                        <option value="tuic">TUIC</option>
                        <option value="naive">Naive</option>
                        <option value="anytls">AnyTLS</option>
                        <option value="wire_guard">WireGuard</option>
                      </select>
                    </Field>
                    <Field label="地址">
                      <input value={editingProfile.server} onChange={(event) => updateProfileDraft((profile) => { profile.server = event.target.value })} />
                    </Field>
                    <Field label="端口">
                      <input type="number" value={editingProfile.port} onChange={(event) => updateProfileDraft((profile) => { profile.port = Number(event.target.value) || 0 })} />
                    </Field>
                    <Field label="UUID / 用户 ID">
                      <input value={editingProfile.uuid ?? ''} onChange={(event) => updateProfileDraft((profile) => { profile.uuid = event.target.value })} />
                    </Field>
                    <Field label="密码">
                      <input value={editingProfile.password ?? ''} onChange={(event) => updateProfileDraft((profile) => { profile.password = event.target.value })} />
                    </Field>
                    <Field label="加密方法">
                      <input value={editingProfile.method ?? ''} onChange={(event) => updateProfileDraft((profile) => { profile.method = event.target.value })} />
                    </Field>
                    <Field label="网络">
                      <select value={editingProfile.network} onChange={(event) => updateProfileDraft((profile) => { profile.network = event.target.value })}>
                        <option value="tcp">tcp</option>
                        <option value="ws">ws</option>
                        <option value="grpc">grpc</option>
                      </select>
                    </Field>
                    <Field label="安全层">
                      <select value={editingProfile.security} onChange={(event) => updateProfileDraft((profile) => { profile.security = event.target.value; profile.tls = event.target.value !== 'none' })}>
                        <option value="none">none</option>
                        <option value="tls">tls</option>
                        <option value="reality">reality</option>
                      </select>
                    </Field>
                    <Field label="SNI">
                      <input value={editingProfile.sni ?? ''} onChange={(event) => updateProfileDraft((profile) => { profile.sni = event.target.value })} />
                    </Field>
                    <Field label="Host / Header">
                      <input value={editingProfile.host ?? ''} onChange={(event) => updateProfileDraft((profile) => { profile.host = event.target.value })} />
                    </Field>
                    <Field label="Path">
                      <input value={editingProfile.path ?? ''} onChange={(event) => updateProfileDraft((profile) => { profile.path = event.target.value })} />
                    </Field>
                    <Field label="gRPC Service Name">
                      <input value={editingProfile.service_name ?? ''} onChange={(event) => updateProfileDraft((profile) => { profile.service_name = event.target.value })} />
                    </Field>
                    <Field label="Flow">
                      <input value={editingProfile.flow ?? ''} onChange={(event) => updateProfileDraft((profile) => { profile.flow = event.target.value })} />
                    </Field>
                    <Field label="Fingerprint">
                      <input value={editingProfile.fingerprint ?? ''} onChange={(event) => updateProfileDraft((profile) => { profile.fingerprint = event.target.value })} />
                    </Field>
                    <Field label="Reality Public Key">
                      <input value={editingProfile.reality_public_key ?? ''} onChange={(event) => updateProfileDraft((profile) => { profile.reality_public_key = event.target.value })} />
                    </Field>
                    <Field label="Reality Short ID">
                      <input value={editingProfile.reality_short_id ?? ''} onChange={(event) => updateProfileDraft((profile) => { profile.reality_short_id = event.target.value })} />
                    </Field>
                      </>
                    )}
                  </div>
                ) : null}
              </SectionCard>
            </div>
          ) : null}

          {activeTab === 'advanced' && professionalMode && advancedTab === 'subscriptions' ? (
            <SectionCard
              title="订阅管理"
              action={
                <div className="flex gap-2">
                  <button className="rounded-xl border border-slate-700 px-3 py-2 text-sm" onClick={addSubscription}>新增订阅</button>
                  <button
                    className="rounded-xl border border-slate-700 px-3 py-2 text-sm"
                    onClick={() => {
                      setBusyAction('subscription-refresh-all')
                      void flushPendingConfigIfNeeded()
                        .then(() => desktopApi.refreshAllSubscriptionsInBackground(recommendedCore))
                        .catch((error) => {
                          setBusyAction(null)
                          setMessage(String(error))
                        })
                    }}
                  >
                    {busyAction === 'subscription-refresh-all' ? '刷新中...' : '刷新全部'}
                  </button>
                </div>
              }
            >
              <div className="space-y-4">
                {config.subscriptions.length === 0 ? (
                  <div className="rounded-2xl border border-dashed border-slate-700 p-6 text-sm text-slate-400">
                    还没有订阅。你可以新增订阅 URL，然后用 sing-box 或 Xray 解析导入。
                  </div>
                ) : null}
                {config.subscriptions.map((subscription) => (
                  <div key={subscription.id} className="rounded-2xl border border-slate-800 bg-slate-900/60 p-4">
                    <div className="grid gap-4 md:grid-cols-2">
                      <Field label="订阅名称">
                        <input
                          value={subscription.name}
                          onChange={(event) => {
                            const next = { ...subscription, name: event.target.value }
                            updateConfig((draft) => {
                              const target = draft.subscriptions.find((item) => item.id === subscription.id)
                              if (target) Object.assign(target, next)
                            })
                          }}
                        />
                      </Field>
                      <Field label="订阅地址">
                        <input
                          value={subscription.url}
                          onChange={(event) => {
                            const next = { ...subscription, url: event.target.value }
                            updateConfig((draft) => {
                              const target = draft.subscriptions.find((item) => item.id === subscription.id)
                              if (target) Object.assign(target, next)
                            })
                          }}
                        />
                      </Field>
                      <Field label="附加 URL（逗号分隔）">
                        <input
                          value={subscription.more_urls.join(',')}
                          onChange={(event) => {
                            const next = {
                              ...subscription,
                              more_urls: event.target.value
                                .split(',')
                                .map((item) => item.trim())
                                .filter(Boolean),
                            }
                            updateConfig((draft) => {
                              const target = draft.subscriptions.find((item) => item.id === subscription.id)
                              if (target) Object.assign(target, next)
                            })
                          }}
                        />
                      </Field>
                      <Field label="User-Agent">
                        <input
                          value={subscription.user_agent}
                          onChange={(event) => {
                            const next = { ...subscription, user_agent: event.target.value }
                            updateConfig((draft) => {
                              const target = draft.subscriptions.find((item) => item.id === subscription.id)
                              if (target) Object.assign(target, next)
                            })
                          }}
                        />
                      </Field>
                      <Field label="节点过滤正则">
                        <input
                          value={subscription.filter ?? ''}
                          onChange={(event) => {
                            const next = { ...subscription, filter: event.target.value }
                            updateConfig((draft) => {
                              const target = draft.subscriptions.find((item) => item.id === subscription.id)
                              if (target) Object.assign(target, next)
                            })
                          }}
                        />
                      </Field>
                      <Field label="自动更新间隔（分钟）">
                        <input
                          type="number"
                          value={subscription.auto_update_interval_secs ?? ''}
                          onChange={(event) => {
                            const value = Number(event.target.value)
                            const next = {
                              ...subscription,
                              auto_update_interval_secs:
                                Number.isFinite(value) && value > 0 ? value : null,
                            }
                            updateConfig((draft) => {
                              const target = draft.subscriptions.find((item) => item.id === subscription.id)
                              if (target) Object.assign(target, next)
                            })
                          }}
                        />
                      </Field>
                      <Field label="转换目标核心">
                        <select
                          value={subscription.convert_core_target ?? ''}
                          onChange={(event) => {
                            const next = {
                              ...subscription,
                              convert_core_target: (event.target.value || null) as Subscription['convert_core_target'],
                            }
                            updateConfig((draft) => {
                              const target = draft.subscriptions.find((item) => item.id === subscription.id)
                              if (target) Object.assign(target, next)
                            })
                          }}
                        >
                          <option value="">跟随刷新按钮</option>
                          <option value="sing_box">sing-box</option>
                          <option value="xray">Xray</option>
                          <option value="mihomo">mihomo</option>
                        </select>
                      </Field>
                      <Field label="通过代理刷新">
                        <select
                          value={String(subscription.use_proxy_on_refresh)}
                          onChange={(event) => {
                            const next = {
                              ...subscription,
                              use_proxy_on_refresh: event.target.value === 'true',
                            }
                            updateConfig((draft) => {
                              const target = draft.subscriptions.find((item) => item.id === subscription.id)
                              if (target) Object.assign(target, next)
                            })
                          }}
                        >
                          <option value="true">true</option>
                          <option value="false">false</option>
                        </select>
                      </Field>
                    </div>
                    <div className="mt-4 flex flex-wrap items-end gap-2">
                      <div className="flex items-end gap-2">
                        <ActionButton busy={busyAction === 'subscription-save'} onClick={() => void handleSaveSubscription(subscription)}>
                          立即保存
                        </ActionButton>
                        <ActionButton busy={busyAction === `subscription-refresh-${subscription.id}`} onClick={() => void handleRefreshSubscription(subscription.id, subscription.convert_core_target ?? recommendedCore)}>
                          刷新
                        </ActionButton>
                        <button
                          className="rounded-xl border border-slate-700 px-3 py-2 text-sm text-slate-200"
                          onClick={() => {
                            void runWithFlushedConfig(() => desktopApi.removeSubscription(subscription.id))
                              .then((nextConfig) => {
                                setConfig(nextConfig)
                              })
                              .catch((error) => setMessage(String(error)))
                          }}
                        >
                          删除
                        </button>
                      </div>
                    </div>
                    <p className="mt-3 text-xs text-slate-400">最近检查：{subscription.last_checked_at ?? '未检查'}</p>
                    <p className="mt-1 text-xs text-slate-400">最近同步：{subscription.last_synced_at ?? '未同步'}</p>
                    {subscription.last_error ? (
                      <p className="mt-1 text-xs text-rose-300">最近错误：{subscription.last_error}</p>
                    ) : null}
                  </div>
                ))}
              </div>
            </SectionCard>
          ) : null}

          {activeTab === 'advanced' && professionalMode && advancedTab === 'settings' ? (
            <div className="grid gap-5 lg:grid-cols-[1fr_1fr]">
              <SectionCard title="本地代理端口">
                <div className="grid gap-4 md:grid-cols-2">
                  <Field label="HTTP 端口">
                    <input type="number" value={config.proxy.http_port} onChange={(event) => updateConfig((draft) => { draft.proxy.http_port = Number(event.target.value) || 0 })} />
                  </Field>
                  <Field label="SOCKS 端口">
                    <input type="number" value={config.proxy.socks_port} onChange={(event) => updateConfig((draft) => { draft.proxy.socks_port = Number(event.target.value) || 0 })} />
                  </Field>
                  <Field label="Mixed 端口">
                    <input type="number" value={config.proxy.mixed_port} onChange={(event) => updateConfig((draft) => { draft.proxy.mixed_port = Number(event.target.value) || 0 })} />
                  </Field>
                  <Field label="绕过域名">
                    <input
                      value={config.proxy.bypass_domains.join(',')}
                      onChange={(event) => updateConfig((draft) => { draft.proxy.bypass_domains = event.target.value.split(',').map((item) => item.trim()).filter(Boolean) })}
                    />
                  </Field>
                </div>
                <div className="mt-4 flex gap-3">
                  <ActionButton
                    busy={busyAction === 'proxy-on' || busyAction === 'proxy-off'}
                    onClick={() => void handleSystemProxy(!config.proxy.use_system_proxy)}
                  >
                    {config.proxy.use_system_proxy ? '关闭系统代理' : '开启系统代理'}
                  </ActionButton>
                </div>
              </SectionCard>

              <SectionCard title="TUN 与网络">
                <div className="grid gap-4 md:grid-cols-2">
                  <Field label="启用 TUN">
                    <select value={String(config.tun.enabled)} onChange={(event) => updateConfig((draft) => { draft.tun.enabled = event.target.value === 'true' })}>
                      <option value="false">false</option>
                      <option value="true">true</option>
                    </select>
                  </Field>
                  <Field label="接口名">
                    <input value={config.tun.interface_name} onChange={(event) => updateConfig((draft) => { draft.tun.interface_name = event.target.value })} />
                  </Field>
                  <Field label="MTU">
                    <input type="number" value={config.tun.mtu} onChange={(event) => updateConfig((draft) => { draft.tun.mtu = Number(event.target.value) || 0 })} />
                  </Field>
                  <Field label="Stack">
                    <input value={config.tun.stack} onChange={(event) => updateConfig((draft) => { draft.tun.stack = event.target.value })} />
                  </Field>
                  <Field label="远程 DNS">
                    <input value={config.dns.remote_dns} onChange={(event) => updateConfig((draft) => { draft.dns.remote_dns = event.target.value })} />
                  </Field>
                  <Field label="直连 DNS">
                    <input value={config.dns.direct_dns} onChange={(event) => updateConfig((draft) => { draft.dns.direct_dns = event.target.value })} />
                  </Field>
                  <Field label="路由模式">
                    <select value={config.routing.mode} onChange={(event) => updateConfig((draft) => { draft.routing.mode = event.target.value })}>
                      <option value="rule">rule</option>
                      <option value="global">global</option>
                      <option value="direct">direct</option>
                    </select>
                  </Field>
                </div>
              </SectionCard>

              <SectionCard title="Mux">
                <div className="grid gap-4 md:grid-cols-2">
                  <Field label="全局启用">
                    <select value={String(config.mux.enabled)} onChange={(event) => updateConfig((draft) => { draft.mux.enabled = event.target.value === 'true' })}>
                      <option value="false">false</option>
                      <option value="true">true</option>
                    </select>
                  </Field>
                  <Field label="sing-box 协议">
                    <select value={config.mux.sing_box_protocol} onChange={(event) => updateConfig((draft) => { draft.mux.sing_box_protocol = event.target.value })}>
                      <option value="h2mux">h2mux</option>
                      <option value="smux">smux</option>
                      <option value="yamux">yamux</option>
                      <option value="">禁用</option>
                    </select>
                  </Field>
                  <Field label="sing-box 最大连接数">
                    <input type="number" value={config.mux.sing_box_max_connections} onChange={(event) => updateConfig((draft) => { draft.mux.sing_box_max_connections = Number(event.target.value) || 0 })} />
                  </Field>
                  <Field label="sing-box padding">
                    <select value={String(config.mux.sing_box_padding ?? '')} onChange={(event) => updateConfig((draft) => { draft.mux.sing_box_padding = event.target.value === '' ? null : event.target.value === 'true' })}>
                      <option value="">default</option>
                      <option value="true">true</option>
                      <option value="false">false</option>
                    </select>
                  </Field>
                  <Field label="Xray TCP 并发">
                    <input type="number" value={config.mux.xray_concurrency ?? ''} onChange={(event) => updateConfig((draft) => { draft.mux.xray_concurrency = Number(event.target.value) || null })} />
                  </Field>
                  <Field label="Xray XUDP 并发">
                    <input type="number" value={config.mux.xray_xudp_concurrency ?? ''} onChange={(event) => updateConfig((draft) => { draft.mux.xray_xudp_concurrency = Number(event.target.value) || null })} />
                  </Field>
                  <Field label="Xray UDP443 策略">
                    <input value={config.mux.xray_xudp_proxy_udp_443 ?? ''} onChange={(event) => updateConfig((draft) => { draft.mux.xray_xudp_proxy_udp_443 = event.target.value })} />
                  </Field>
                </div>
              </SectionCard>

              <SectionCard title="Clash API">
                <div className="grid gap-4 md:grid-cols-2">
                  <Field label="external-controller 端口">
                    <input type="number" value={config.clash.external_controller_port} onChange={(event) => updateConfig((draft) => { draft.clash.external_controller_port = Number(event.target.value) || 0 })} />
                  </Field>
                  <Field label="bind-address">
                    <input value={config.clash.bind_address} onChange={(event) => updateConfig((draft) => { draft.clash.bind_address = event.target.value })} />
                  </Field>
                  <Field label="allow-lan">
                    <select value={String(config.clash.allow_lan)} onChange={(event) => updateConfig((draft) => { draft.clash.allow_lan = event.target.value === 'true' })}>
                      <option value="false">false</option>
                      <option value="true">true</option>
                    </select>
                  </Field>
                  <Field label="启用 IPv6">
                    <select value={String(config.clash.enable_ipv6)} onChange={(event) => updateConfig((draft) => { draft.clash.enable_ipv6 = event.target.value === 'true' })}>
                      <option value="false">false</option>
                      <option value="true">true</option>
                    </select>
                  </Field>
                  <Field label="Rule Mode">
                    <select value={config.clash.rule_mode} onChange={(event) => updateConfig((draft) => { draft.clash.rule_mode = event.target.value })}>
                      <option value="rule">rule</option>
                      <option value="global">global</option>
                      <option value="direct">direct</option>
                      <option value="unchanged">unchanged</option>
                    </select>
                  </Field>
                  <Field label="secret">
                    <input value={config.clash.secret ?? ''} onChange={(event) => updateConfig((draft) => { draft.clash.secret = event.target.value })} />
                  </Field>
                  <Field label="启用 Mixin">
                    <select value={String(config.clash.enable_mixin_content)} onChange={(event) => updateConfig((draft) => { draft.clash.enable_mixin_content = event.target.value === 'true' })}>
                      <option value="false">false</option>
                      <option value="true">true</option>
                    </select>
                  </Field>
                  <Field label="代理组自动刷新">
                    <select value={String(config.clash.proxies_auto_refresh)} onChange={(event) => updateConfig((draft) => { draft.clash.proxies_auto_refresh = event.target.value === 'true' })}>
                      <option value="false">false</option>
                      <option value="true">true</option>
                    </select>
                  </Field>
                  <Field label="代理组排序">
                    <select value={String(config.clash.proxies_sorting)} onChange={(event) => updateConfig((draft) => { draft.clash.proxies_sorting = Number(event.target.value) || 0 })}>
                      <option value="0">按延迟</option>
                      <option value="1">按名称</option>
                      <option value="2">保持原序</option>
                    </select>
                  </Field>
                  <Field label="代理组测速间隔（分钟）">
                    <input type="number" value={config.clash.proxies_auto_delay_test_interval} onChange={(event) => updateConfig((draft) => { draft.clash.proxies_auto_delay_test_interval = Number(event.target.value) || 0 })} />
                  </Field>
                  <Field label="测速 URL">
                    <input value={config.clash.proxies_auto_delay_test_url} onChange={(event) => updateConfig((draft) => { draft.clash.proxies_auto_delay_test_url = event.target.value })} />
                  </Field>
                  <Field label="Provider 自动刷新">
                    <select value={String(config.clash.providers_auto_refresh)} onChange={(event) => updateConfig((draft) => { draft.clash.providers_auto_refresh = event.target.value === 'true' })}>
                      <option value="false">false</option>
                      <option value="true">true</option>
                    </select>
                  </Field>
                  <Field label="Provider 刷新间隔（分钟）">
                    <input type="number" value={config.clash.providers_refresh_interval} onChange={(event) => updateConfig((draft) => { draft.clash.providers_refresh_interval = Number(event.target.value) || 0 })} />
                  </Field>
                  <Field label="连接自动刷新">
                    <select value={String(config.clash.connections_auto_refresh)} onChange={(event) => updateConfig((draft) => { draft.clash.connections_auto_refresh = event.target.value === 'true' })}>
                      <option value="false">false</option>
                      <option value="true">true</option>
                    </select>
                  </Field>
                  <Field label="连接刷新间隔（5秒步长）">
                    <input type="number" value={config.clash.connections_refresh_interval} onChange={(event) => updateConfig((draft) => { draft.clash.connections_refresh_interval = Number(event.target.value) || 0 })} />
                  </Field>
                </div>
                <div className="mt-4">
                  <Field label="Mixin YAML">
                    <textarea className="min-h-40 w-full bg-transparent font-mono text-xs outline-none" value={config.clash.mixin_content} onChange={(event) => updateConfig((draft) => { draft.clash.mixin_content = event.target.value })} />
                  </Field>
                </div>
              </SectionCard>
            </div>
          ) : null}

          {activeTab === 'advanced' && professionalMode && advancedTab === 'routing' ? (
            <div className="grid gap-5 xl:grid-cols-[0.9fr_1.1fr_1.2fr]">
              <SectionCard
                title="路由集"
                action={
                  <div className="flex gap-2">
                    <ActionButton busy={busyAction === 'routing-init'} onClick={() => void handleInitializeBuiltinRouting(false)}>
                      初始化
                    </ActionButton>
                    <ActionButton busy={false} onClick={addRoutingItem}>
                      新增
                    </ActionButton>
                  </div>
                }
              >
                <div className="grid gap-4">
                  <Field label="全局 Domain Strategy">
                    <select value={config.routing.domain_strategy} onChange={(event) => updateConfig((draft) => { draft.routing.domain_strategy = event.target.value })}>
                      <option value="AsIs">AsIs</option>
                      <option value="IPIfNonMatch">IPIfNonMatch</option>
                      <option value="IPOnDemand">IPOnDemand</option>
                    </select>
                  </Field>
                  <Field label="全局 sing-box Strategy">
                    <select value={config.routing.domain_strategy_4_singbox} onChange={(event) => updateConfig((draft) => { draft.routing.domain_strategy_4_singbox = event.target.value })}>
                      <option value="">default</option>
                      <option value="prefer_ipv4">prefer_ipv4</option>
                      <option value="prefer_ipv6">prefer_ipv6</option>
                      <option value="ipv4_only">ipv4_only</option>
                      <option value="ipv6_only">ipv6_only</option>
                    </select>
                  </Field>
                  <Field label="模板 URL">
                    <input value={routingTemplateUrlDraft} onChange={(event) => setRoutingTemplateUrlDraft(event.target.value)} />
                  </Field>
                  <div className="flex flex-wrap gap-2">
                    <ActionButton busy={busyAction === 'routing-template-url'} onClick={() => void handleImportRoutingTemplateUrl()}>
                      导入模板 URL
                    </ActionButton>
                    <ActionButton busy={busyAction === 'routing-init'} onClick={() => void handleInitializeBuiltinRouting(true)}>
                      追加内置模板
                    </ActionButton>
                    <ActionButton busy={busyAction === 'routing-export'} onClick={() => void handleExportRoutingRules()}>
                      导出规则
                    </ActionButton>
                  </div>
                  <div className="space-y-3">
                    {config.routing.items.map((item) => (
                      <button
                        key={item.id}
                        className={`w-full rounded-2xl border px-4 py-3 text-left ${selectedRoutingId === item.id ? 'border-violet-400 bg-violet-500/10' : 'border-slate-800 bg-slate-950/70'}`}
                        onClick={() => setSelectedRoutingId(item.id)}
                      >
                        <div className="flex items-center justify-between gap-3">
                          <div>
                            <p className="font-medium">{item.remarks || '未命名路由集'}</p>
                            <p className="mt-1 text-xs text-slate-400">规则数：{item.rule_num} · {item.is_active ? '默认集' : '非默认'}</p>
                          </div>
                          <div className="flex gap-2">
                            <button
                              className="rounded-xl border border-slate-700 px-3 py-1 text-xs"
                              onClick={(event) => {
                                event.stopPropagation()
                                void runWithFlushedConfig(() => desktopApi.setDefaultRoutingItem(item.id)).then((nextConfig) => {
                                  setConfig(nextConfig)
                                  setMessage('默认路由集已切换')
                                  setPreviewStale(true)
                                }).catch((error) => setMessage(String(error)))
                              }}
                            >
                              设默认
                            </button>
                            <button
                              className="rounded-xl border border-rose-800 px-3 py-1 text-xs text-rose-300"
                              onClick={(event) => {
                                event.stopPropagation()
                                void runWithFlushedConfig(() => desktopApi.removeRoutingItem(item.id)).then((nextConfig) => {
                                  setConfig(nextConfig)
                                  setMessage('路由集已删除')
                                }).catch((error) => setMessage(String(error)))
                              }}
                            >
                              删除
                            </button>
                          </div>
                        </div>
                      </button>
                    ))}
                  </div>
                </div>
              </SectionCard>

              <SectionCard
                title="规则列表"
                action={
                  <div className="flex gap-2">
                    <ActionButton busy={false} onClick={addRoutingRule}>
                      新增规则
                    </ActionButton>
                    <ActionButton
                      busy={busyAction === 'routing-import'}
                      onClick={() => {
                        const raw = window.prompt('粘贴路由规则 JSON')
                        if (!raw || !selectedRouting) {
                          return
                        }
                        setBusyAction('routing-import')
                        void flushPendingConfigIfNeeded()
                          .then(() => desktopApi.importRoutingRules(selectedRouting.id, raw, false))
                          .then((nextConfig) => {
                            setConfig(nextConfig)
                            setMessage('路由规则已导入')
                            setPreviewStale(true)
                          })
                          .catch((error) => setMessage(String(error)))
                          .finally(() => setBusyAction(null))
                      }}
                    >
                      导入 JSON
                    </ActionButton>
                  </div>
                }
              >
                {!editingRouting ? <div className="text-sm text-slate-400">请先选择左侧路由集。</div> : null}
                {editingRouting ? (
                  <div className="grid gap-4">
                    <Field label="备注">
                      <input value={editingRouting.remarks} onChange={(event) => updateRoutingDraft((item) => {
                        item.remarks = event.target.value
                      })} />
                    </Field>
                    <Field label="自定义 ruleset 路径">
                      <input value={editingRouting.custom_ruleset_path_4_singbox ?? ''} onChange={(event) => updateRoutingDraft((item) => {
                        item.custom_ruleset_path_4_singbox = event.target.value
                      })} />
                    </Field>
                    <div className="space-y-2">
                      {editingRouting.rule_set.map((rule) => (
                        <button
                          key={rule.id}
                          className={`w-full rounded-2xl border px-4 py-3 text-left ${selectedRoutingRuleId === rule.id ? 'border-violet-400 bg-violet-500/10' : 'border-slate-800 bg-slate-950/70'}`}
                          onClick={() => setSelectedRoutingRuleId(rule.id)}
                        >
                          <p className="font-medium">{rule.remarks || '未命名规则'}</p>
                          <p className="mt-1 text-xs text-slate-400">
                            {rule.rule_type} · {rule.outbound_tag || 'proxy'} · {(rule.domain[0] || rule.ip[0] || rule.process[0] || rule.port || '-')}
                          </p>
                          <div className="mt-2 flex gap-2">
                            <button
                              className="rounded-xl border border-slate-700 px-3 py-1 text-xs"
                              onClick={(event) => {
                                event.stopPropagation()
                                if (!selectedRouting) return
                                void runWithFlushedConfig(() => desktopApi.moveRoutingRule(selectedRouting.id, rule.id, 'up')).then((nextConfig) => {
                                  setConfig(nextConfig)
                                }).catch((error) => setMessage(String(error)))
                              }}
                            >
                              上移
                            </button>
                            <button
                              className="rounded-xl border border-slate-700 px-3 py-1 text-xs"
                              onClick={(event) => {
                                event.stopPropagation()
                                if (!selectedRouting) return
                                void runWithFlushedConfig(() => desktopApi.moveRoutingRule(selectedRouting.id, rule.id, 'down')).then((nextConfig) => {
                                  setConfig(nextConfig)
                                }).catch((error) => setMessage(String(error)))
                              }}
                            >
                              下移
                            </button>
                          </div>
                        </button>
                      ))}
                    </div>
                  </div>
                ) : null}
              </SectionCard>

              <SectionCard title="规则详情">
                {!editingRoutingRule ? <div className="text-sm text-slate-400">请先选择一条规则。</div> : null}
                {editingRouting && editingRoutingRule ? (
                  <div className="grid gap-4 md:grid-cols-2">
                    <Field label="备注">
                      <input value={editingRoutingRule.remarks ?? ''} onChange={(event) => updateRoutingRuleDraft((rule) => {
                        rule.remarks = event.target.value
                      })} />
                    </Field>
                    <Field label="启用">
                      <select value={String(editingRoutingRule.enabled)} onChange={(event) => updateRoutingRuleDraft((rule) => {
                        rule.enabled = event.target.value === 'true'
                      })}>
                        <option value="true">true</option>
                        <option value="false">false</option>
                      </select>
                    </Field>
                    <Field label="RuleType">
                      <select value={editingRoutingRule.rule_type} onChange={(event) => updateRoutingRuleDraft((rule) => {
                        rule.rule_type = event.target.value as RoutingRule['rule_type']
                      })}>
                        <option value="all">all</option>
                        <option value="routing">routing</option>
                        <option value="dns">dns</option>
                      </select>
                    </Field>
                    <Field label="Outbound">
                      <select value={editingRoutingRule.outbound_tag ?? 'proxy'} onChange={(event) => updateRoutingRuleDraft((rule) => {
                        rule.outbound_tag = event.target.value
                      })}>
                        <option value="proxy">proxy</option>
                        <option value="direct">direct</option>
                        <option value="block">block</option>
                      </select>
                    </Field>
                    <Field label="Domain">
                      <textarea className="min-h-24 w-full bg-transparent outline-none" value={editingRoutingRule.domain.join('\n')} onChange={(event) => updateRoutingRuleDraft((rule) => {
                        rule.domain = event.target.value.split('\n').map((item) => item.trim()).filter(Boolean)
                      })} />
                    </Field>
                    <Field label="IP">
                      <textarea className="min-h-24 w-full bg-transparent outline-none" value={editingRoutingRule.ip.join('\n')} onChange={(event) => updateRoutingRuleDraft((rule) => {
                        rule.ip = event.target.value.split('\n').map((item) => item.trim()).filter(Boolean)
                      })} />
                    </Field>
                    <Field label="Process">
                      <textarea className="min-h-24 w-full bg-transparent outline-none" value={editingRoutingRule.process.join('\n')} onChange={(event) => updateRoutingRuleDraft((rule) => {
                        rule.process = event.target.value.split('\n').map((item) => item.trim()).filter(Boolean)
                      })} />
                    </Field>
                    <Field label="Protocol">
                      <input value={editingRoutingRule.protocol.join(',')} onChange={(event) => updateRoutingRuleDraft((rule) => {
                        rule.protocol = event.target.value.split(',').map((item) => item.trim()).filter(Boolean)
                      })} />
                    </Field>
                    <Field label="Port">
                      <input value={editingRoutingRule.port ?? ''} onChange={(event) => updateRoutingRuleDraft((rule) => {
                        rule.port = event.target.value
                      })} />
                    </Field>
                    <Field label="Network">
                      <input value={editingRoutingRule.network ?? ''} onChange={(event) => updateRoutingRuleDraft((rule) => {
                        rule.network = event.target.value
                      })} />
                    </Field>
                    <Field label="InboundTag">
                      <input value={editingRoutingRule.inbound_tag.join(',')} onChange={(event) => updateRoutingRuleDraft((rule) => {
                        rule.inbound_tag = event.target.value.split(',').map((item) => item.trim()).filter(Boolean)
                      })} />
                    </Field>
                  </div>
                ) : null}
              </SectionCard>
            </div>
          ) : null}

          {activeTab === 'advanced' && professionalMode && advancedTab === 'clash' ? (
            status.runtime.running && status.runtime.core_type === 'mihomo' ? (
              <div className="grid gap-5 xl:grid-cols-[0.85fr_1fr_1fr]">
                <SectionCard
                  title="配置与 Provider"
                  action={
                    <div className="flex gap-2">
                      <ActionButton busy={busyAction === 'clash-rule-mode'} onClick={() => void handleUpdateClashRuleMode(config.clash.rule_mode)}>
                        应用模式
                      </ActionButton>
                      <ActionButton busy={busyAction === 'clash-reload'} onClick={() => void handleReloadClashConfig()}>
                        热重载
                      </ActionButton>
                    </div>
                  }
                >
                  <div className="grid gap-4">
                    <Field label="运行时 Rule Mode">
                      <select value={config.clash.rule_mode} onChange={(event) => updateConfig((draft) => { draft.clash.rule_mode = event.target.value })}>
                        <option value="rule">rule</option>
                        <option value="global">global</option>
                        <option value="direct">direct</option>
                        <option value="unchanged">unchanged</option>
                      </select>
                    </Field>
                    <div className="space-y-3">
                      <p className="text-sm text-slate-400">Providers</p>
                      {clashProxyProviders.length === 0 ? <div className="text-sm text-slate-500">暂无 provider 数据</div> : null}
                      {clashProxyProviders.map((provider) => (
                        <div key={provider.name} className="rounded-2xl border border-slate-800 bg-slate-950/70 p-4">
                          <p className="font-medium">{provider.name}</p>
                          <p className="mt-1 text-xs text-slate-400">{provider.provider_type} · {provider.vehicle_type ?? 'unknown'}</p>
                          <p className="mt-1 text-xs text-slate-400">节点数：{provider.proxies.length} · 更新时间：{provider.updated_at ?? '未知'}</p>
                          <ActionButton
                            className="mt-3"
                            busy={busyAction === `clash-provider-${provider.name}`}
                            onClick={() => void handleRefreshClashProvider(provider.name)}
                          >
                            刷新 Provider
                          </ActionButton>
                        </div>
                      ))}
                    </div>
                  </div>
                </SectionCard>
                <SectionCard
                  title="代理组"
                  action={
                    <div className="flex gap-2">
                      <ActionButton busy={busyAction === 'clash-refresh'} onClick={() => void refreshClashState()}>
                        刷新
                      </ActionButton>
                      <ActionButton busy={busyAction === 'clash-refresh'} onClick={() => void refreshClashState(true)}>
                        测速
                      </ActionButton>
                    </div>
                  }
                >
                  <div className="space-y-4">
                    {sortedClashProxyGroups.length === 0 ? <div className="text-sm text-slate-400">暂无代理组数据</div> : null}
                    {sortedClashProxyGroups.map((group) => (
                      <div key={group.name} className="rounded-2xl border border-slate-800 bg-slate-950/70 p-4">
                        <p className="font-medium">{group.name}</p>
                        <p className="mt-1 text-xs text-slate-400">{group.proxy_type}</p>
                        <p className="mt-1 text-xs text-slate-400">最近延迟：{group.last_delay_ms ? `${group.last_delay_ms} ms` : '未知'}</p>
                        <div className="mt-3 flex gap-3">
                          <select
                            className="w-full rounded-xl border border-slate-700 bg-slate-950 px-3 py-2 text-sm"
                            value={group.now ?? ''}
                            onChange={(event) => void handleClashProxySelect(group.name, event.target.value)}
                            disabled={!['Selector', 'URLTest', 'Fallback', 'LoadBalance'].includes(group.proxy_type)}
                          >
                            {group.all.map((name) => (
                              <option key={name} value={name}>
                                {name}
                              </option>
                            ))}
                          </select>
                        </div>
                      </div>
                    ))}
                  </div>
                </SectionCard>
                <SectionCard
                  title="连接"
                  action={
                    <div className="flex gap-2">
                      <ActionButton busy={busyAction === 'clash-refresh'} onClick={() => void refreshClashState()}>
                        刷新
                      </ActionButton>
                      <ActionButton busy={busyAction === 'clash-close-all'} onClick={() => void handleCloseClashConnection('')}>
                        关闭全部
                      </ActionButton>
                    </div>
                  }
                >
                  <div className="space-y-3 text-sm text-slate-300">
                    <Field label="连接过滤">
                      <input value={clashConnectionFilter} onChange={(event) => setClashConnectionFilter(event.target.value)} />
                    </Field>
                    {filteredClashConnections.length === 0 ? <div className="text-sm text-slate-400">暂无连接数据</div> : null}
                    {filteredClashConnections.map((connection) => (
                      <div key={connection.id} className="rounded-2xl border border-slate-800 bg-slate-950/70 p-4">
                        <p className="font-mono text-xs text-slate-500">{connection.id}</p>
                        <p className="mt-2">{connection.host ?? connection.destination ?? '-'}</p>
                        <p className="mt-1 text-xs text-slate-400">
                          {connection.rule ?? '-'} · {connection.chains.join(' -> ') || '-'}
                        </p>
                        <p className="mt-1 text-xs text-slate-500">
                          ↑ {formatBytes(connection.upload)} / ↓ {formatBytes(connection.download)}
                        </p>
                        <ActionButton
                          className="mt-3"
                          busy={busyAction === `clash-close-${connection.id}`}
                          onClick={() => void handleCloseClashConnection(connection.id)}
                        >
                          关闭连接
                        </ActionButton>
                      </div>
                    ))}
                  </div>
                </SectionCard>
              </div>
            ) : (
              <SectionCard title="Clash">
                <div className="text-sm text-slate-400">仅在运行 `mihomo` 外部配置时显示代理组和连接信息。</div>
              </SectionCard>
            )
          ) : null}

          {activeTab === 'advanced' && professionalMode && advancedTab === 'logs' ? (
            <SectionCard title="核心日志">
              <div className="mb-4 flex items-center justify-between text-sm text-slate-400">
                <span>共 {logs.length} 条日志</span>
                <button className="rounded-xl border border-slate-700 px-3 py-2" onClick={() => setLogs([])}>
                  清空
                </button>
              </div>
              <div className="max-h-[42rem] overflow-auto rounded-2xl bg-slate-950 p-4 font-mono text-xs leading-6">
                {logs.length === 0 ? <div className="text-slate-500">启动核心后会在这里显示 stdout / stderr。</div> : null}
                {logs.map((log, index) => (
                  <div key={`${log.source}-${index}`} className="border-b border-slate-900 py-1 text-slate-300">
                    <span className={log.level === 'error' ? 'text-rose-300' : 'text-emerald-300'}>[{log.source}]</span>{' '}
                    {log.message}
                  </div>
                ))}
              </div>
            </SectionCard>
          ) : null}
        </main>
      </div>
    </div>
  )
}

function SectionCard({
  title,
  action,
  children,
}: {
  title: string
  action?: ReactNode
  children: ReactNode
}) {
  return (
    <section className="rounded-3xl border border-slate-800 bg-slate-900/70 p-5 shadow-2xl shadow-slate-950/30">
      <div className="mb-4 flex items-center justify-between gap-3">
        <h3 className="text-lg font-semibold">{title}</h3>
        {action}
      </div>
      {children}
    </section>
  )
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="block">
      <span className="mb-2 block text-sm text-slate-400">{label}</span>
      <div className="rounded-2xl border border-slate-700 bg-slate-950 px-3 py-2 text-sm text-slate-100 [&_input]:w-full [&_input]:bg-transparent [&_input]:outline-none [&_select]:w-full [&_select]:bg-transparent [&_select]:outline-none [&_textarea]:w-full [&_textarea]:bg-transparent [&_textarea]:outline-none">
        {children}
      </div>
    </label>
  )
}

function KeyValue({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div>
      <p className="text-xs uppercase tracking-wide text-slate-500">{label}</p>
      <p className={`mt-1 break-all ${mono ? 'font-mono text-xs' : ''}`}>{value}</p>
    </div>
  )
}

function CoreCard({
  asset,
  busy,
  recommended,
  onDownload,
}: {
  asset: CoreAssetStatus
  busy: boolean
  recommended?: boolean
  onDownload: () => void
}) {
  return (
    <div className="rounded-2xl border border-slate-800 bg-slate-950/70 p-4">
      <div className="flex items-center justify-between gap-3">
        <p className="text-lg font-medium text-slate-100">{formatCoreType(asset.core_type)}</p>
        {recommended ? <span className="rounded-full bg-violet-500/20 px-2 py-1 text-xs text-violet-100">推荐</span> : null}
      </div>
      <p className="mt-2 text-sm text-slate-400">已安装：{asset.installed_version ?? '未安装'}</p>
      <p className="mt-1 text-sm text-slate-400">最新：{asset.latest_version ?? '未知'}</p>
      <p className="mt-2 break-all font-mono text-[11px] text-slate-500">{asset.executable_path ?? '尚无可执行文件'}</p>
      <ActionButton busy={busy} onClick={onDownload} className="mt-4 w-full justify-center">
        下载 / 更新
      </ActionButton>
    </div>
  )
}

function QuickStepCard({
  index,
  title,
  description,
  done,
  action,
}: {
  index: number
  title: string
  description: string
  done: boolean
  action?: ReactNode
}) {
  return (
    <div className="rounded-2xl border border-slate-800 bg-slate-950/70 p-4">
      <div className="flex items-center justify-between gap-3">
        <span className="rounded-full bg-slate-800 px-2 py-1 text-xs text-slate-300">步骤 {index}</span>
        <StatusPill label={done ? '已完成' : '待完成'} tone={done ? 'success' : 'muted'} />
      </div>
      <p className="mt-3 text-base font-medium text-slate-100">{title}</p>
      <p className="mt-2 text-sm text-slate-400">{description}</p>
      {action ? <div className="mt-4">{action}</div> : null}
    </div>
  )
}

function ModeOptionCard({
  title,
  description,
  active,
  onClick,
}: {
  title: string
  description: string
  active: boolean
  onClick: () => void
}) {
  return (
    <button
      className={`rounded-2xl border px-4 py-4 text-left transition ${
        active ? 'border-violet-500/50 bg-violet-500/10' : 'border-slate-800 bg-slate-950/70 hover:border-slate-700'
      }`}
      onClick={onClick}
    >
      <p className="font-medium text-slate-100">{title}</p>
      <p className="mt-2 text-sm text-slate-400">{description}</p>
    </button>
  )
}

function SummaryStat({ title, value, description }: { title: string; value: string; description: string }) {
  return (
    <div className="rounded-2xl border border-slate-800 bg-slate-950/70 p-4">
      <p className="text-xs uppercase tracking-wide text-slate-500">{title}</p>
      <p className="mt-2 text-xl font-semibold text-slate-100">{value}</p>
      <p className="mt-2 text-sm text-slate-400">{description}</p>
    </div>
  )
}

function StatusPill({
  label,
  tone,
}: {
  label: string
  tone: 'success' | 'muted' | 'warning'
}) {
  const palette =
    tone === 'success'
      ? 'bg-emerald-500/10 text-emerald-200'
      : tone === 'warning'
        ? 'bg-amber-500/10 text-amber-100'
        : 'bg-slate-800 text-slate-300'
  return <span className={`inline-flex items-center rounded-full px-3 py-1 text-xs ${palette}`}>{label}</span>
}

function EmptyState({
  title,
  description,
  action,
}: {
  title: string
  description: string
  action?: ReactNode
}) {
  return (
    <div className="rounded-2xl border border-dashed border-slate-700 p-6 text-sm text-slate-400">
      <p className="font-medium text-slate-200">{title}</p>
      <p className="mt-2">{description}</p>
      {action ? <div className="mt-4">{action}</div> : null}
    </div>
  )
}

function SimpleRuleCard({
  title,
  action,
  remarks,
  onRemove,
}: {
  title: string
  action: string
  remarks: string
  onRemove: () => void
}) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-3 rounded-2xl border border-slate-800 bg-slate-950/70 p-4">
      <div>
        <p className="font-medium text-slate-100">{title}</p>
        <p className="mt-1 text-xs text-slate-500">{remarks || '普通模式规则'}</p>
      </div>
      <div className="flex items-center gap-3">
        <StatusPill label={formatRuleAction(action)} tone={action === 'block' ? 'warning' : 'success'} />
        <button className="rounded-xl border border-slate-700 px-3 py-2 text-sm text-slate-200" onClick={onRemove}>
          删除
        </button>
      </div>
    </div>
  )
}

function ActionButton({
  busy,
  onClick,
  className,
  children,
}: {
  busy?: boolean
  onClick: () => void
  className?: string
  children: ReactNode
}) {
  return (
    <button
      className={`inline-flex items-center rounded-xl bg-violet-500 px-4 py-2 text-sm font-medium text-white transition hover:bg-violet-400 disabled:cursor-not-allowed disabled:bg-slate-700 ${className ?? ''}`}
      onClick={onClick}
      disabled={busy}
    >
      {busy ? '处理中...' : children}
    </button>
  )
}

function formatBytes(value?: number | null) {
  if (!value) {
    return '0 B'
  }
  if (value < 1024) {
    return `${value} B`
  }
  if (value < 1024 * 1024) {
    return `${(value / 1024).toFixed(1)} KB`
  }
  return `${(value / (1024 * 1024)).toFixed(1)} MB`
}

function formatCoreType(coreType: CoreType) {
  if (coreType === 'sing_box') {
    return 'sing-box'
  }
  if (coreType === 'mihomo') {
    return 'mihomo'
  }
  return 'Xray'
}

function getModeLabel(mode: QuickMode) {
  if (mode === 'global') {
    return '全局代理'
  }
  if (mode === 'direct') {
    return '直接连接'
  }
  return '智能分流'
}

function formatRuleAction(action: string) {
  if (action === 'direct') {
    return '直接连接'
  }
  if (action === 'block') {
    return '阻止'
  }
  return '走代理'
}

function formatImportFormat(format: ImportPreview['format']) {
  switch (format) {
    case 'share_links':
      return '分享链接 / 订阅内容'
    case 'sing_box_json':
      return 'sing-box JSON'
    case 'xray_json':
      return 'Xray JSON'
    case 'clash_yaml':
      return 'Clash YAML'
    default:
      return '未知内容'
  }
}

function isHttpUrl(raw: string) {
  const text = raw.trim().toLowerCase()
  return text.startsWith('http://') || text.startsWith('https://')
}

function looksLikeImportText(raw: string) {
  const text = raw.trim().toLowerCase()
  if (!text) {
    return false
  }
  return (
    isHttpUrl(text) ||
    text.startsWith('vless://') ||
    text.startsWith('vmess://') ||
    text.startsWith('trojan://') ||
    text.startsWith('ss://') ||
    text.startsWith('hysteria2://') ||
    text.startsWith('hy2://') ||
    text.startsWith('tuic://') ||
    text.startsWith('wireguard://') ||
    text.startsWith('anytls://') ||
    text.includes('proxies:') ||
    text.startsWith('{') ||
    text.startsWith('[')
  )
}

async function mapWithConcurrency<T, R>(
  items: T[],
  concurrency: number,
  mapper: (item: T, index: number) => Promise<R>,
): Promise<R[]> {
  if (items.length === 0) {
    return []
  }

  const results = new Array<R>(items.length)
  let nextIndex = 0
  const workerCount = Math.max(1, Math.min(concurrency, items.length))

  await Promise.all(
    Array.from({ length: workerCount }, async () => {
      while (nextIndex < items.length) {
        const currentIndex = nextIndex
        nextIndex += 1
        results[currentIndex] = await mapper(items[currentIndex], currentIndex)
      }
    }),
  )

  return results
}

export default App

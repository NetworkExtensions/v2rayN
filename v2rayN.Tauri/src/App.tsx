import type { ReactNode } from 'react'
import { useEffect, useMemo, useState } from 'react'

import { desktopApi, runtimeMode } from './lib/api'
import type {
  AppConfig,
  AppStatus,
  CoreAssetStatus,
  CoreLogEvent,
  CoreType,
  Profile,
  Subscription,
} from './lib/types'

type TabKey = 'overview' | 'profiles' | 'subscriptions' | 'settings' | 'logs'

const tabs: Array<{ key: TabKey; label: string }> = [
  { key: 'overview', label: '总览' },
  { key: 'profiles', label: '节点' },
  { key: 'subscriptions', label: '订阅' },
  { key: 'settings', label: '设置' },
  { key: 'logs', label: '日志' },
]

function App() {
  const [activeTab, setActiveTab] = useState<TabKey>('overview')
  const [status, setStatus] = useState<AppStatus | null>(null)
  const [config, setConfig] = useState<AppConfig | null>(null)
  const [selectedProfileId, setSelectedProfileId] = useState<string>('')
  const [logs, setLogs] = useState<CoreLogEvent[]>([])
  const [importText, setImportText] = useState('')
  const [preview, setPreview] = useState('')
  const [busyAction, setBusyAction] = useState<string | null>(null)
  const [message, setMessage] = useState<string>('')
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    void loadStatus()
    const unlistenPromise = desktopApi.onCoreLog((event) => {
      setLogs((current) => [...current.slice(-499), event])
    })

    return () => {
      void unlistenPromise.then((unlisten) => unlisten())
    }
  }, [])

  const selectedProfile = useMemo(() => {
    return config?.profiles.find((profile) => profile.id === selectedProfileId) ?? null
  }, [config, selectedProfileId])

  async function loadStatus() {
    try {
      const nextStatus = await desktopApi.getStatus()
      setStatus(nextStatus)
      setConfig(nextStatus.config)
      setSelectedProfileId(nextStatus.config.selected_profile_id ?? nextStatus.config.profiles[0]?.id ?? '')
      const generated = await desktopApi.generatePreview()
      setPreview(generated)
      if (runtimeMode === 'browser') {
        setMessage('当前运行在浏览器模式，使用本地 mock 数据；若要连接真实 Rust 后端，请使用 `npm run tauri dev`。')
      }
    } catch (error) {
      setMessage(String(error))
    } finally {
      setLoading(false)
    }
  }

  async function persistConfig(nextConfig: AppConfig, successMessage?: string) {
    setBusyAction('save')
    try {
      const saved = await desktopApi.saveConfig(nextConfig)
      setConfig(saved)
      setSelectedProfileId(saved.selected_profile_id ?? saved.profiles[0]?.id ?? '')
      setMessage(successMessage ?? '配置已保存')
      setPreview(await desktopApi.generatePreview())
    } catch (error) {
      setMessage(String(error))
    } finally {
      setBusyAction(null)
    }
  }

  function updateConfig(mutator: (draft: AppConfig) => void) {
    if (!config) {
      return
    }

    const draft: AppConfig = structuredClone(config)
    mutator(draft)
    setConfig(draft)
  }

  function updateSelectedProfile(mutator: (profile: Profile) => void) {
    updateConfig((draft) => {
      const target = draft.profiles.find((profile) => profile.id === selectedProfileId)
      if (target) {
        mutator(target)
      }
    })
  }

  async function handleImport(coreType: CoreType) {
    if (!importText.trim()) {
      setMessage('请输入分享链接或订阅内容')
      return
    }

    setBusyAction('import')
    try {
      const nextConfig = await desktopApi.importShareLinks(importText, coreType)
      setConfig(nextConfig)
      setSelectedProfileId(nextConfig.selected_profile_id ?? nextConfig.profiles[0]?.id ?? '')
      setImportText('')
      setMessage('导入成功')
      setPreview(await desktopApi.generatePreview())
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
      const nextConfig = await desktopApi.refreshSubscription(subscriptionId, coreType)
      setConfig(nextConfig)
      setMessage('订阅刷新完成')
      setPreview(await desktopApi.generatePreview())
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
      setMessage(`${coreType} 下载完成`)
    } catch (error) {
      setMessage(String(error))
    } finally {
      setBusyAction(null)
    }
  }

  async function handleStart() {
    setBusyAction('start')
    try {
      const runtime = await desktopApi.startCore()
      setStatus((current) => (current ? { ...current, runtime } : current))
      setMessage('核心已启动')
      setPreview(await desktopApi.generatePreview())
    } catch (error) {
      setMessage(String(error))
    } finally {
      setBusyAction(null)
    }
  }

  async function handleStop() {
    setBusyAction('stop')
    try {
      const runtime = await desktopApi.stopCore()
      setStatus((current) => (current ? { ...current, runtime } : current))
      setMessage('核心已停止')
    } catch (error) {
      setMessage(String(error))
    } finally {
      setBusyAction(null)
    }
  }

  async function handleSystemProxy(enabled: boolean) {
    setBusyAction(enabled ? 'proxy-on' : 'proxy-off')
    try {
      const nextConfig = enabled
        ? await desktopApi.enableSystemProxy()
        : await desktopApi.disableSystemProxy()
      setConfig(nextConfig)
      setMessage(enabled ? '系统代理已开启' : '系统代理已关闭')
    } catch (error) {
      setMessage(String(error))
    } finally {
      setBusyAction(null)
    }
  }

  if (loading) {
    return <div className="flex min-h-screen items-center justify-center bg-slate-950 text-slate-100">正在加载应用状态...</div>
  }

  if (!config || !status) {
    return (
      <div className="flex min-h-screen items-center justify-center bg-slate-950 px-6 text-slate-100">
        <div className="max-w-xl rounded-3xl border border-rose-500/30 bg-slate-900 p-6 shadow-2xl shadow-slate-950/30">
          <h1 className="text-xl font-semibold">应用状态加载失败</h1>
          <p className="mt-3 text-sm text-slate-300">{message || '未获取到初始化状态。'}</p>
          <p className="mt-3 text-sm text-slate-400">
            如果你是在浏览器里直接跑 `npm run dev`，建议改为 `npm run tauri dev`，或等待浏览器模式 fallback 生效后再刷新页面。
          </p>
          <button
            className="mt-5 rounded-xl bg-violet-500 px-4 py-2 text-sm font-medium text-white"
            onClick={() => {
              setLoading(true)
              setMessage('')
              void loadStatus()
            }}
          >
            重新加载
          </button>
        </div>
      </div>
    )
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
      last_synced_at: null,
    }
    updateConfig((draft) => {
      draft.subscriptions.push(subscription)
    })
  }

  return (
    <div className="min-h-screen bg-slate-950 text-slate-100">
      <div className="mx-auto flex min-h-screen max-w-[1600px]">
        <aside className="w-60 border-r border-slate-800 bg-slate-900/80 p-5">
          <div className="mb-8">
            <p className="text-xs uppercase tracking-[0.3em] text-violet-300">v2rayN</p>
            <h1 className="mt-3 text-2xl font-semibold">Tauri 重构版</h1>
            <p className="mt-2 text-sm text-slate-400">Rust 负责运行时，React 负责交互与可视化。</p>
            <span className="mt-3 inline-flex rounded-full border border-slate-700 px-2 py-1 text-[11px] text-slate-300">
              {runtimeMode === 'tauri' ? 'Tauri Runtime' : 'Browser Mock Runtime'}
            </span>
          </div>
          <nav className="space-y-2">
            {tabs.map((tab) => (
              <button
                key={tab.key}
                className={`w-full rounded-xl px-3 py-2 text-left text-sm transition ${
                  activeTab === tab.key
                    ? 'bg-violet-500/20 text-violet-200'
                    : 'text-slate-300 hover:bg-slate-800 hover:text-white'
                }`}
                onClick={() => setActiveTab(tab.key)}
              >
                {tab.label}
              </button>
            ))}
          </nav>
          <div className="mt-8 rounded-2xl border border-slate-800 bg-slate-950/70 p-4 text-xs text-slate-400">
            <p>数据目录</p>
            <p className="mt-2 break-all font-mono text-[11px] text-slate-300">{status.paths.root}</p>
          </div>
        </aside>

        <main className="flex-1 overflow-auto p-6">
          <header className="mb-6 flex flex-wrap items-center justify-between gap-4 rounded-3xl border border-slate-800 bg-slate-900/70 px-5 py-4">
            <div>
              <h2 className="text-xl font-semibold">{tabs.find((tab) => tab.key === activeTab)?.label}</h2>
              <p className="mt-1 text-sm text-slate-400">
                当前节点：{selectedProfile?.name ?? '未选择'} · 运行状态：
                <span className={status.runtime.running ? 'text-emerald-300' : 'text-slate-300'}>
                  {status.runtime.running ? ' 已启动' : ' 未启动'}
                </span>
              </p>
            </div>
            <div className="flex flex-wrap gap-3">
              <button className="rounded-xl bg-violet-500 px-4 py-2 text-sm font-medium text-white" onClick={handleStart}>
                启动核心
              </button>
              <button className="rounded-xl border border-slate-700 px-4 py-2 text-sm text-slate-200" onClick={handleStop}>
                停止核心
              </button>
              <button
                className="rounded-xl border border-slate-700 px-4 py-2 text-sm text-slate-200"
                onClick={() => void persistConfig(config, '配置已保存')}
              >
                保存配置
              </button>
            </div>
          </header>

          {message ? (
            <div className="mb-5 rounded-2xl border border-violet-500/30 bg-violet-500/10 px-4 py-3 text-sm text-violet-100">
              {message}
            </div>
          ) : null}

          {activeTab === 'overview' ? (
            <div className="grid gap-5 lg:grid-cols-[1.35fr_1fr]">
              <SectionCard title="核心安装状态">
                <div className="grid gap-4 md:grid-cols-2">
                  {status.core_assets.map((asset) => (
                    <CoreCard
                      key={asset.core_type}
                      asset={asset}
                      installDirectory={`${status.paths.bin}/${asset.core_type}`}
                      busy={busyAction === `download-${asset.core_type}`}
                      onDownload={() => void handleCoreDownload(asset.core_type)}
                    />
                  ))}
                </div>
              </SectionCard>

              <SectionCard title="运行时概览">
                <div className="space-y-3 text-sm text-slate-300">
                  <KeyValue label="核心类型" value={status.runtime.core_type ?? '未启动'} />
                  <KeyValue label="配置文件" value={status.runtime.config_path ?? '-'} mono />
                  <KeyValue label="执行文件" value={status.runtime.executable_path ?? '-'} mono />
                  <KeyValue label="系统代理" value={config.proxy.use_system_proxy ? '已开启' : '未开启'} />
                  <KeyValue label="TUN 模式" value={config.tun.enabled ? '已开启' : '未开启'} />
                </div>
              </SectionCard>

              <SectionCard title="配置预览">
                <pre className="max-h-[34rem] overflow-auto rounded-2xl bg-slate-950 p-4 text-xs text-slate-200">
                  {preview}
                </pre>
              </SectionCard>

              <SectionCard title="快速导入">
                <textarea
                  value={importText}
                  onChange={(event) => setImportText(event.target.value)}
                  className="h-60 w-full rounded-2xl border border-slate-700 bg-slate-950 px-4 py-3 text-sm outline-none"
                  placeholder="粘贴 vless:// vmess:// trojan:// ss:// 分享链接，一行一个"
                />
                <div className="mt-4 flex gap-3">
                  <ActionButton busy={busyAction === 'import'} onClick={() => void handleImport('sing_box')}>
                    导入为 sing-box 节点
                  </ActionButton>
                  <ActionButton busy={busyAction === 'import'} onClick={() => void handleImport('xray')}>
                    导入为 Xray 节点
                  </ActionButton>
                </div>
              </SectionCard>
            </div>
          ) : null}

          {activeTab === 'profiles' ? (
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
                        {profile.protocol} · {profile.core_type}
                      </p>
                      <p className="mt-2 text-sm text-slate-400">
                        {profile.server}:{profile.port}
                      </p>
                    </button>
                  ))}
                </div>
              </SectionCard>

              <SectionCard title="节点编辑">
                {selectedProfile ? (
                  <div className="grid gap-4 md:grid-cols-2">
                    <Field label="节点名称">
                      <input value={selectedProfile.name} onChange={(event) => updateSelectedProfile((profile) => { profile.name = event.target.value })} />
                    </Field>
                    <Field label="核心">
                      <select value={selectedProfile.core_type} onChange={(event) => updateSelectedProfile((profile) => { profile.core_type = event.target.value as CoreType })}>
                        <option value="sing_box">sing-box</option>
                        <option value="xray">Xray</option>
                      </select>
                    </Field>
                    <Field label="协议">
                      <select value={selectedProfile.protocol} onChange={(event) => updateSelectedProfile((profile) => { profile.protocol = event.target.value as Profile['protocol'] })}>
                        <option value="vless">VLESS</option>
                        <option value="vmess">VMess</option>
                        <option value="trojan">Trojan</option>
                        <option value="shadowsocks">Shadowsocks</option>
                      </select>
                    </Field>
                    <Field label="地址">
                      <input value={selectedProfile.server} onChange={(event) => updateSelectedProfile((profile) => { profile.server = event.target.value })} />
                    </Field>
                    <Field label="端口">
                      <input type="number" value={selectedProfile.port} onChange={(event) => updateSelectedProfile((profile) => { profile.port = Number(event.target.value) || 0 })} />
                    </Field>
                    <Field label="UUID / 用户 ID">
                      <input value={selectedProfile.uuid ?? ''} onChange={(event) => updateSelectedProfile((profile) => { profile.uuid = event.target.value })} />
                    </Field>
                    <Field label="密码">
                      <input value={selectedProfile.password ?? ''} onChange={(event) => updateSelectedProfile((profile) => { profile.password = event.target.value })} />
                    </Field>
                    <Field label="加密方法">
                      <input value={selectedProfile.method ?? ''} onChange={(event) => updateSelectedProfile((profile) => { profile.method = event.target.value })} />
                    </Field>
                    <Field label="网络">
                      <select value={selectedProfile.network} onChange={(event) => updateSelectedProfile((profile) => { profile.network = event.target.value })}>
                        <option value="tcp">tcp</option>
                        <option value="ws">ws</option>
                        <option value="grpc">grpc</option>
                      </select>
                    </Field>
                    <Field label="安全层">
                      <select value={selectedProfile.security} onChange={(event) => updateSelectedProfile((profile) => { profile.security = event.target.value; profile.tls = event.target.value !== 'none' })}>
                        <option value="none">none</option>
                        <option value="tls">tls</option>
                        <option value="reality">reality</option>
                      </select>
                    </Field>
                    <Field label="SNI">
                      <input value={selectedProfile.sni ?? ''} onChange={(event) => updateSelectedProfile((profile) => { profile.sni = event.target.value })} />
                    </Field>
                    <Field label="Host / Header">
                      <input value={selectedProfile.host ?? ''} onChange={(event) => updateSelectedProfile((profile) => { profile.host = event.target.value })} />
                    </Field>
                    <Field label="Path">
                      <input value={selectedProfile.path ?? ''} onChange={(event) => updateSelectedProfile((profile) => { profile.path = event.target.value })} />
                    </Field>
                    <Field label="gRPC Service Name">
                      <input value={selectedProfile.service_name ?? ''} onChange={(event) => updateSelectedProfile((profile) => { profile.service_name = event.target.value })} />
                    </Field>
                    <Field label="Flow">
                      <input value={selectedProfile.flow ?? ''} onChange={(event) => updateSelectedProfile((profile) => { profile.flow = event.target.value })} />
                    </Field>
                    <Field label="Fingerprint">
                      <input value={selectedProfile.fingerprint ?? ''} onChange={(event) => updateSelectedProfile((profile) => { profile.fingerprint = event.target.value })} />
                    </Field>
                    <Field label="Reality Public Key">
                      <input value={selectedProfile.reality_public_key ?? ''} onChange={(event) => updateSelectedProfile((profile) => { profile.reality_public_key = event.target.value })} />
                    </Field>
                    <Field label="Reality Short ID">
                      <input value={selectedProfile.reality_short_id ?? ''} onChange={(event) => updateSelectedProfile((profile) => { profile.reality_short_id = event.target.value })} />
                    </Field>
                  </div>
                ) : null}
              </SectionCard>
            </div>
          ) : null}

          {activeTab === 'subscriptions' ? (
            <SectionCard title="订阅管理" action={<button className="rounded-xl border border-slate-700 px-3 py-2 text-sm" onClick={addSubscription}>新增订阅</button>}>
              <div className="space-y-4">
                {config.subscriptions.length === 0 ? (
                  <div className="rounded-2xl border border-dashed border-slate-700 p-6 text-sm text-slate-400">
                    还没有订阅。你可以新增订阅 URL，然后用 sing-box 或 Xray 解析导入。
                  </div>
                ) : null}
                {config.subscriptions.map((subscription) => (
                  <div key={subscription.id} className="rounded-2xl border border-slate-800 bg-slate-900/60 p-4">
                    <div className="grid gap-4 md:grid-cols-[1fr_2fr_auto]">
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
                      <div className="flex items-end gap-2">
                        <ActionButton busy={busyAction === 'subscription-save'} onClick={() => void handleSaveSubscription(subscription)}>
                          保存
                        </ActionButton>
                        <ActionButton busy={busyAction === `subscription-refresh-${subscription.id}`} onClick={() => void handleRefreshSubscription(subscription.id, 'sing_box')}>
                          刷新
                        </ActionButton>
                      </div>
                    </div>
                    <p className="mt-3 text-xs text-slate-400">最近同步：{subscription.last_synced_at ?? '未同步'}</p>
                  </div>
                ))}
              </div>
            </SectionCard>
          ) : null}

          {activeTab === 'settings' ? (
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
                  <ActionButton busy={busyAction === 'proxy-on'} onClick={() => void handleSystemProxy(true)}>
                    开启系统代理
                  </ActionButton>
                  <ActionButton busy={busyAction === 'proxy-off'} onClick={() => void handleSystemProxy(false)}>
                    关闭系统代理
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
            </div>
          ) : null}

          {activeTab === 'logs' ? (
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
      <div className="rounded-2xl border border-slate-700 bg-slate-950 px-3 py-2 text-sm text-slate-100 [&_input]:w-full [&_input]:bg-transparent [&_input]:outline-none [&_select]:w-full [&_select]:bg-transparent [&_select]:outline-none">
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
  installDirectory,
  busy,
  onDownload,
}: {
  asset: CoreAssetStatus
  installDirectory: string
  busy: boolean
  onDownload: () => void
}) {
  return (
    <div className="rounded-2xl border border-slate-800 bg-slate-950/70 p-4">
      <p className="text-lg font-medium text-slate-100">
        {asset.core_type === 'sing_box' ? 'sing-box' : 'Xray'}
      </p>
      <p className="mt-2 text-sm text-slate-400">已安装：{asset.installed_version ?? '未安装'}</p>
      <p className="mt-1 text-sm text-slate-400">最新：{asset.latest_version ?? '未知'}</p>
      <p className="mt-3 text-xs text-slate-500">下载地址</p>
      <p className="mt-1 break-all font-mono text-[11px] text-slate-400">{asset.download_url ?? '尚未解析到下载链接'}</p>
      <p className="mt-3 text-xs text-slate-500">安装目录</p>
      <p className="mt-1 break-all font-mono text-[11px] text-slate-400">{installDirectory}</p>
      <p className="mt-2 break-all font-mono text-[11px] text-slate-500">{asset.executable_path ?? '尚无可执行文件'}</p>
      <ActionButton busy={busy} onClick={onDownload} className="mt-4 w-full justify-center">
        下载 / 更新
      </ActionButton>
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

export default App

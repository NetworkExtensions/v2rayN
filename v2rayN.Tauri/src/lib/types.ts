export type CoreType = 'xray' | 'sing_box'
export type ProfileProtocol =
  | 'vless'
  | 'vmess'
  | 'trojan'
  | 'shadowsocks'
  | 'hysteria2'
  | 'tuic'
  | 'wire_guard'
  | 'naive'
  | 'anytls'

export interface Profile {
  id: string
  name: string
  core_type: CoreType
  protocol: ProfileProtocol
  server: string
  port: number
  uuid?: string | null
  password?: string | null
  method?: string | null
  network: string
  security: string
  tls: boolean
  sni?: string | null
  host?: string | null
  path?: string | null
  service_name?: string | null
  flow?: string | null
  fingerprint?: string | null
  reality_public_key?: string | null
  reality_short_id?: string | null
  alpn: string[]
  udp: boolean
}

export interface Subscription {
  id: string
  name: string
  url: string
  enabled: boolean
  last_synced_at?: string | null
}

export interface ProxySettings {
  http_port: number
  socks_port: number
  mixed_port: number
  bypass_domains: string[]
  use_system_proxy: boolean
}

export interface TunSettings {
  enabled: boolean
  interface_name: string
  mtu: number
  auto_route: boolean
  strict_route: boolean
  stack: string
}

export interface DnsSettings {
  remote_dns: string
  direct_dns: string
}

export interface RoutingSettings {
  mode: string
}

export interface AppConfig {
  selected_profile_id?: string | null
  profiles: Profile[]
  subscriptions: Subscription[]
  proxy: ProxySettings
  tun: TunSettings
  dns: DnsSettings
  routing: RoutingSettings
}

export interface AppPaths {
  root: string
  bin: string
  bin_configs: string
  gui_logs: string
  state_file: string
}

export interface RunningStatus {
  running: boolean
  core_type?: CoreType | null
  profile_id?: string | null
  executable_path?: string | null
  config_path?: string | null
  pid?: number | null
  elevated: boolean
  helper_core_type?: CoreType | null
  helper_config_path?: string | null
  helper_pid?: number | null
}

export interface CoreAssetStatus {
  core_type: CoreType
  installed_version?: string | null
  latest_version?: string | null
  download_url?: string | null
  executable_path?: string | null
}

export interface AppStatus {
  paths: AppPaths
  config: AppConfig
  runtime: RunningStatus
  core_assets: CoreAssetStatus[]
  proxy_probe?: ProxyProbe | null
}

export interface CoreLogEvent {
  level: string
  source: string
  message: string
}

export interface ProxyProbe {
  outbound_ip: string
  country?: string | null
  city?: string | null
  isp?: string | null
}

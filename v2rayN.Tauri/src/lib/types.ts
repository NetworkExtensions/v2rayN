export type CoreType = 'xray' | 'sing_box' | 'mihomo'
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

export type ProfileConfigType = 'native' | 'external'
export type ExternalConfigFormat = 'sing_box' | 'xray' | 'clash'
export type MuxOverride = 'follow_global' | 'force_enable' | 'force_disable'
export type RoutingRuleType = 'all' | 'routing' | 'dns'

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
  mux_override: MuxOverride
  source_subscription_id?: string | null
  config_type: ProfileConfigType
  external_config_format?: ExternalConfigFormat | null
  external_config_path?: string | null
}

export interface Subscription {
  id: string
  name: string
  url: string
  enabled: boolean
  more_urls: string[]
  user_agent: string
  filter?: string | null
  auto_update_interval_secs?: number | null
  convert_core_target?: CoreType | null
  use_proxy_on_refresh: boolean
  last_checked_at?: string | null
  last_synced_at?: string | null
  last_error?: string | null
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
  domain_strategy: string
  domain_strategy_4_singbox: string
  routing_index_id?: string | null
  template_source_url?: string | null
  items: RoutingItem[]
}

export interface RoutingRule {
  id: string
  rule_type: RoutingRuleType
  enabled: boolean
  remarks?: string | null
  type_name?: string | null
  port?: string | null
  network?: string | null
  inbound_tag: string[]
  outbound_tag?: string | null
  ip: string[]
  domain: string[]
  protocol: string[]
  process: string[]
}

export interface RoutingItem {
  id: string
  remarks: string
  url: string
  rule_set: RoutingRule[]
  rule_num: number
  enabled: boolean
  locked: boolean
  custom_icon?: string | null
  custom_ruleset_path_4_singbox?: string | null
  domain_strategy?: string | null
  domain_strategy_4_singbox?: string | null
  sort: number
  is_active: boolean
}

export interface RoutingTemplate {
  version: string
  routing_items: RoutingItem[]
}

export interface MuxSettings {
  enabled: boolean
  xray_concurrency?: number | null
  xray_xudp_concurrency?: number | null
  xray_xudp_proxy_udp_443?: string | null
  sing_box_protocol: string
  sing_box_max_connections: number
  sing_box_padding?: boolean | null
}

export interface ClashSettings {
  external_controller_port: number
  enable_ipv6: boolean
  allow_lan: boolean
  bind_address: string
  rule_mode: string
  secret?: string | null
  enable_mixin_content: boolean
  mixin_content: string
  proxies_sorting: number
  proxies_auto_refresh: boolean
  proxies_auto_delay_test_interval: number
  proxies_auto_delay_test_url: string
  providers_auto_refresh: boolean
  providers_refresh_interval: number
  connections_auto_refresh: boolean
  connections_refresh_interval: number
}

export interface AppConfig {
  selected_profile_id?: string | null
  profiles: Profile[]
  subscriptions: Subscription[]
  proxy: ProxySettings
  tun: TunSettings
  dns: DnsSettings
  routing: RoutingSettings
  mux: MuxSettings
  clash: ClashSettings
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

export interface BackgroundTaskEvent {
  task: string
  success: boolean
  message: string
}

export interface ProxyProbe {
  outbound_ip: string
  country?: string | null
  city?: string | null
  isp?: string | null
}

export type ImportFormat = 'share_links' | 'sing_box_json' | 'xray_json' | 'clash_yaml' | 'unknown'

export interface ImportPreview {
  format: ImportFormat
  profile_names: string[]
  profile_count: number
  stores_as_external: boolean
  external_format?: ExternalConfigFormat | null
  message?: string | null
}

export interface ClashProxyGroup {
  name: string
  proxy_type: string
  now?: string | null
  all: string[]
  last_delay_ms?: number | null
}

export interface ClashConnection {
  id: string
  network?: string | null
  type?: string | null
  rule?: string | null
  chains: string[]
  upload?: number | null
  download?: number | null
  host?: string | null
  destination?: string | null
  start?: string | null
}

export interface ClashProxyProvider {
  name: string
  provider_type: string
  vehicle_type?: string | null
  updated_at?: string | null
  proxies: string[]
}

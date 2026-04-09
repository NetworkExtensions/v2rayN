# v2rayN (Tauri Rewrite)

基于 `Tauri + Rust + React` 的重写版本，支持桌面模式和浏览器模式两种运行方式。

当前已支持通过界面控制：

- 代理核心（启动 / 停止 / 重启 / 日志 / 状态）
- 节点与订阅管理
- 路由与 Clash 相关能力
- 网络相关控制（系统代理切换、连通性探测等）

> 说明：系统代理切换目前主要实现为 macOS 场景。

## 项目结构

- 前端：`v2rayN.Tauri`（React + TypeScript + Vite）
- 后端：`v2rayN.Tauri/src-tauri`（Rust + Tauri）
- 浏览器模式后端入口：`cargo run --bin server`

## 环境要求

- Node.js 20+
- npm
- Rust（建议 stable）
- Tauri 开发依赖（仅桌面模式需要）

## 一、桌面模式（Tauri）

### 开发运行

```bash
cd v2rayN.Tauri
npm install
npm run tauri dev
```

### 构建

```bash
cd v2rayN.Tauri
npm install
npm run tauri build
```

## 二、网页模式（Browser + Local Server）

网页模式下，前端通过本地 HTTP/WebSocket 与 Rust 服务通信，默认地址：

- 前端：`http://localhost:1420`
- 后端 API：`http://127.0.0.1:7393`

### 开发运行

终端 A（启动后端服务）：

```bash
cd v2rayN.Tauri/src-tauri
cargo run --bin server
```

终端 B（启动前端）：

```bash
cd v2rayN.Tauri
npm install
npm run dev
```

### 自定义后端端口

```bash
cd v2rayN.Tauri/src-tauri
cargo run --bin server -- --port 8080
```

## 常用检查命令

```bash
# 前端构建检查
cd v2rayN.Tauri
npm run build

# 后端编译检查
cd v2rayN.Tauri/src-tauri
cargo check
```

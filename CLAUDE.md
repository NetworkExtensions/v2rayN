# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working in this repository.

## Project Status

This repository has been rewritten around `v2rayN.Tauri/`.

- Active implementation: `v2rayN.Tauri/` (React + TypeScript frontend, Rust + Tauri backend)
- Legacy C# codebase under `v2rayN/` is deprecated and should not be used for new work
- Browser mode is supported via local HTTP/WebSocket server in Rust (`cargo run --bin server`)

## Main Architecture

### Frontend (`v2rayN.Tauri/`)

- Stack: React + TypeScript + Vite
- Main API adapter: `src/lib/api.ts`
  - Tauri runtime: uses `@tauri-apps/api` invoke/listen
  - Browser runtime: uses local HTTP (`127.0.0.1:7393`) + WebSocket (`/api/events`)
- Main UI entry: `src/App.tsx`

### Backend (`v2rayN.Tauri/src-tauri/`)

- Stack: Rust + Tauri 2
- Core command layer: `src/commands.rs`
- Shared runtime/app state: `src/app_state.rs`
- Event bus: `src/events.rs` (`tokio::sync::broadcast`)
- HTTP bridge for browser mode: `src/http_server.rs`
- Standalone server bin: `src/server.rs`

## Working Rules

- Prefer editing files under `v2rayN.Tauri/`
- Keep Tauri mode and Browser mode behavior consistent
- When adding backend capability:
  1) Implement in `commands.rs` (or domain layer)
  2) Expose via Tauri command
  3) Expose equivalent HTTP route in `http_server.rs` for browser mode
- Keep error handling defensive in frontend (`try/catch` + user-visible message)

## Build and Run

All commands below run from `v2rayN.Tauri/` unless noted.

### Frontend only (browser mode UI)

```bash
npm install
npm run dev
```

### Browser mode full stack

Run backend and frontend in parallel:

```bash
# terminal A
cd v2rayN.Tauri/src-tauri
cargo run --bin server

# terminal B
cd v2rayN.Tauri
npm run dev
```

### Tauri desktop mode

```bash
cd v2rayN.Tauri
npm run tauri dev
```

## Validation Commands

- Frontend type check/build: `npm run build`
- Rust check: `cd src-tauri && cargo check`

## Notes

- HTTP server default port is `7393` on `127.0.0.1`
- If startup fails with `AddrInUse`, terminate previous server process and retry
- Do not reintroduce legacy C# project assumptions into new architecture

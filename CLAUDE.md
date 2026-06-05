# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Moonlight Web is an unofficial [Moonlight](https://moonlight-stream.org/) client that streams a PC running
[Sunshine](https://docs.lizardbyte.dev/projects/sunshine/latest/) to a web browser. A Rust web server forwards
Sunshine traffic to the browser over WebRTC (or WebSocket fallback) using the browser's WebCodecs/WebRTC APIs.

## Repository layout

This is a Cargo workspace (edition 2024) plus a TypeScript frontend:

- **`common/`** (crate `common`) — shared types between web-server and streamer. Contains `config.rs` (the full
  `config.json` schema), `ipc.rs` (the streamer subprocess protocol), and `api_bindings.rs` (REST/WS message types
  annotated with `ts-rs` `#[ts(export)]`).
- **`src/`** (crate `web-server`, the root package, `default-run`) — the Actix-web HTTP/WS server. `api/` holds REST
  handlers, `app/` holds business logic (`App` over a `Storage` trait), `app/storage/json/` is the JSON-file storage
  backend. This is the binary users run.
- **`streamer/`** (crate `streamer`) — a **subprocess** the web-server spawns, one per active stream. It talks to
  `moonlight-common-rust` (the actual Moonlight protocol), receives video/audio, and pushes it to the browser via
  `transport/webrtc/` or `transport/web_socket/`.
- **`web/`** — the TypeScript frontend (no framework; hand-rolled `Component` classes). `web/stream/` is the
  browser-side streaming pipeline (decode + render video/audio, gather input). Build output goes to `dist/`.
- **`docker/`** — Dockerfiles and compose files (including a coturn-bundled variant).

`moonlight-common`, `webrtc-rs`, and `openh264-js` are pinned to forks (see `Cargo.toml` git revs and `.gitmodules`).
Always clone with `--recurse-submodules`.

## Core architecture

**Two-process model.** The web-server never touches the Moonlight protocol directly. For each stream it spawns the
`streamer` binary as a child process and communicates over **stdin/stdout as newline-delimited JSON** (`stderr` is
the child's log stream, forwarded into the parent's tracing spans). The message types are `ServerIpcMessage` (parent
→ child) and `StreamerIpcMessage` (child → parent) in `common/src/ipc.rs`; the IPC plumbing is `create_child_ipc` /
`create_process_ipc` in the same file. The stream lifecycle starts at `src/api/stream.rs` (`/api/host/stream`
WebSocket) which spawns the subprocess and bridges the browser WebSocket to the child's IPC.

**Transports.** Both video/audio and the negotiation can flow over WebRTC (default, peer-to-peer, needs STUN/TURN)
or WebSockets (fallback over the existing HTTP(S) port). The streamer implements both in `streamer/src/transport/`;
the browser side mirrors them in `web/stream/transport/`. WebSocket transport requires a secure context (HTTPS) for
the browser `VideoDecoder`, else it falls back to a slower decoder (or the bundled openh264 WASM build).

**Auth & data model.** Users have a Role, roles carry `permissions` (codec/bitrate/HDR/transport limits) and
`default_settings`. Permissions are enforced both server-side and reflected in the GUI; `apply_permissions_to_settings`
in `common/src/lib.rs` clamps a user's settings to their role. Auth supports password/session tokens, a default user
(no login), and reverse-proxy forwarded-header auth. The first user to log in becomes admin. Storage is behind the
`Storage` trait (`src/app/storage/`), currently only a JSON-file implementation.

## Shared types: ts-rs binding generation

REST and WebSocket message types are defined **once in Rust** (`common/src/api_bindings.rs`) and exported to
TypeScript at `web/api_bindings.ts` via `ts-rs`. **`web/api_bindings.ts` is generated — never edit it by hand.**
After changing any `#[ts(export)]` type, regenerate:

```sh
npm run generate-bindings   # runs: cargo test export_bindings --package common
```

## Common commands

Rust toolchain is **nightly, pinned in `rust-toolchain.toml`** (the streamer uses nightly-only features). The
frontend needs Node (CI uses Node 24).

```sh
# Frontend
npm install
npm run dev            # generate-bindings, then watch-build web/ -> dist/
npm run build          # generate-bindings + tsc + copy static assets -> dist/

# Rust
cargo build                                      # debug build (workspace default members: streamer + web-server)
cargo build --release                            # release
cargo run                                         # run the web-server (reads/creates server/config.json)
cargo run -- help                                 # CLI help (config has CLI args + env vars)
cargo run -- print-config                         # dump the effective config as JSON

# Tests / lint (mirrors CI — run before pushing)
cargo test --workspace --locked
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings   # clippy must be clean; warnings fail CI
cargo shear --expand                              # detects unused dependencies (CI runs this)

# Run a single test
cargo test --package common export_bindings
```

`unwrap_used`, `todo` are clippy `warn` lints at the workspace level — avoid `.unwrap()` and `todo!()` in committed code.

## Running locally

Running the web-server expects a `streamer` binary and the static frontend next to it at runtime:

- The server reads/writes `server/config.json` relative to the working directory (created with defaults on first run).
- The frontend output directory must be named **`dist/` for debug builds** and **`static/` for release builds**
  (the server picks the directory based on build profile). `npm run build` produces `dist/`; release packaging
  renames it to `static/` (see `buildAll.ps1` / `.github/workflows/ci.yml`).
- The streamer subprocess binary must be discoverable by the web-server (built into the same `target/<profile>/` dir
  by `cargo build`).

## Full builds / releases

- `buildAll.ps1` and `build-windows.ps1` are local multi-target build scripts (use `cross`).
- `.github/workflows/ci.yml` runs fmt/clippy/shear/test on every push, then cross-compiles for
  windows-gnu / linux-gnu / linux-musl (x86_64 + aarch64), and on `v*` tags builds release archives and Docker images.
- Cross-compilation uses `cross` (`Cross.toml`); note Windows only targets `x86_64-pc-windows-gnu`.

## Config notes

`common/src/config.rs` is the source of truth for every config option; most options also have a CLI flag and an env
var. The README documents the user-facing subset (bind address, HTTPS certs, WebRTC ICE servers / NAT-1to1 / port
range, URL path prefix for reverse-proxy subpaths, forwarded-header auth). `config.json` is parsed through
`preprocess_human_json` (`src/human_json.rs`), which allows trailing commas / comments.

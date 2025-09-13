![Discord](https://img.shields.io/discord/779512040934342687?label=Discord)

# Factorio Server Manager – Rust Rewrite

This repository now ships a Rust backend (Axum + SeaORM) that replaces the original Go implementation. The frontend under `ui/` remains the same; static assets are served from `app/`. HTTP and WebSocket APIs aim to be compatible with the original so the existing UI works without changes.

Credits
- Based on and inspired by OpenFactorioServerManager/factorio-server-manager (MIT): https://github.com/OpenFactorioServerManager/factorio-server-manager
- Thanks to the original authors and maintainers: Mitch Roote, knoxfighter, Jannaahs, and all contributors

Project Status
- The Go backend (old `src/`) has been removed. The Rust backend is at the repository root (`Cargo.toml` in root).
- Static assets live in `app/`, frontend sources in `ui/`.
- Configuration is `conf.toml` (see `conf.toml.example`).

Features
- API parity with the Go version (REST + WS; compatible with `ui/`)
- Auto-detects `factorio_dir`; generates and persists cookie key, RCON password and port on first run
- Streaming uploads/downloads for saves/mods/mod packs to reduce memory usage
- Mods directory concurrency protection: in‑process RwLock + cross‑process lockfile (`.fsm_mods.lock`)
- WebSocket broadcasting of `server_status` and `gamelog` with snapshot replay on connect
- Convenience: hides `*.tmp.zip`; only passes `--server-adminlist` on Factorio ≥ 0.17

Quick Start (Local HTTP)
1) Extract the official headless server under the repo root at `./factorio` (must include `bin/x64/factorio`, `data/base`, `config`).
2) From repo root run:
   - `FSMR_SECURE=false RUST_LOG=info cargo run`
3) On first start it will:
   - Detect `factorio_dir` and persist `cookie_key_b64`, `factorio_rcon_pass`, `factorio_rcon_port` into `conf.toml`
   - Initialize the database and create a default `admin` (a random password is printed to the console)
4) Open `http://127.0.0.1:3000/login` and log in with `admin + printed password`

Docker
- Quick start from `docker/`:
  - `docker compose -f docker/docker-compose.simple.yaml up -d`
- The container will download Factorio headless on first run. Data/config are persisted under `./docker/fsm-data` and `./docker/factorio-data/*` by default.

Configuration
- Main file: `conf.toml` (example: `conf.toml.example`).
- Environment overrides use the `FSMR_` prefix (e.g. `FSMR_CONF`, `FSMR_BIND_ADDR`, `FSMR_SECURE=false`).
- First run persists generated values (cookie key, RCON password/port) back to `conf.toml`.

Development
- Build: `cargo build`
- Run with verbose logs: `RUST_LOG=debug,tower_http=debug,axum=debug cargo run`
- Frontend bundle: `npm install && npm run build` or `make app/bundle` (outputs to `app/`).

License
- MIT — see `LICENSE.md`. Huge thanks to the original project and all contributors.

Other Languages
- 中文说明请见 `README_ZH.MD`.

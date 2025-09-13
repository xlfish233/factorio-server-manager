# Repository Guidelines

## Project Structure & Module Organization
- 本仓库后端为 Rust 实现（替代原 Go 版），接口尽量与旧版保持一致，以复用前端 `ui/`。
- Rust server (Axum + SeaORM)：入口 `src/main.rs`，配置位于仓库根 `conf.toml`。
- `app/`：由 Rust 服务的静态资源（例如 `index.html`、`bundle.js`）。
- `ui/`：前端源码（构建后产物可放入 `app/` 供后端直出）。
- `factorio/`：运行期使用的游戏数据与二进制（路径通过配置指定）。

## Build, Test, and Development Commands
- Build: `cargo build` — 编译 Axum 服务。
- Run (dev logs): `RUST_LOG=debug,tower_http=debug,axum=info cargo run`
- Config override: `FSMR_CONF=conf.toml` 和 `FSMR_SECURE=false`（本地 HTTP 调试）。
- Database: 默认在仓库根创建 SQLite 文件 `dev.db`。

## Coding Style & Naming Conventions
- Rust: 4‑space indent，遵循 Rust 惯例。路由在 `src/routes/*`，服务在 `src/services/*`，共享状态在 `src/state.rs`。
- Tracing: 使用 `tracing` 宏（`info!`、`debug!`、`warn!`、`trace!`）并包含简洁结构化字段。
- Errors: 气泡式传递；返回 `Result<T, Box<dyn Error>>` 或项目自定义 `AppError`。
- Files and modules: 文件/模块用 snake_case，类型用 CamelCase。

## Testing Guidelines
- 单元测试紧邻代码，使用 `#[cfg(test)]` 模块。
- 优先快速、隔离的测试；非必要避免跨进程用例。
- 全量测试：`cargo test`（仓库根）。

## Commit & Pull Request Guidelines
- Messages: 采用 conventional 提示（如 `feat:`、`fix:`、`refactor:`、`docs:`）。例：`fix(ws): authorize handshake and add trace logs`。
- PRs: 说明背景、动机、行为变化，并附截图/日志片段（若涉及 UI 或行为）。
- 关联相关 issue；描述测试步骤与风险点。

## Security & Configuration Tips
- Cookies: 本地 HTTP 调试将 `secure = false`；生产环境使用 HTTPS 并保持 `secure = true`。
- WebSocket: `/ws` 需要认证；确保登录流设置 `authentication` Cookie。
- Config: 参见仓库根 `conf.toml.example`；可用 `FSMR_` 前缀环境变量覆盖。

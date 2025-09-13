![Discord](https://img.shields.io/discord/779512040934342687?label=Discord)

# Factorio Server Manager – Rust 重构

本仓库现在以 Rust 实现（Axum + SeaORM）为后端，替代原有 Go 实现；前端仍复用 `ui/`，静态产物由后端从 `app/` 提供。HTTP 与 WebSocket 接口尽量保持与旧版兼容，以便前端无需修改即可工作。

致敬与鸣谢
- 基于并致敬原项目 OpenFactorioServerManager/factorio-server-manager（MIT）：https://github.com/OpenFactorioServerManager/factorio-server-manager
- 原作者与维护者：Mitch Roote、knoxfighter、Jannaahs，以及所有贡献者（详见原仓库贡献者）

项目状态
- Go 版后端（原 `src/`）已移除，Rust 版已移至仓库根目录（`Cargo.toml` 位于根目录）。
- 静态资源目录为 `app/`，前端源码在 `ui/`。
- 配置文件为 `conf.toml`（示例：`conf.toml.example`）。

主要特性
- 与 Go 版一致的 REST/WS 接口（兼容 `ui/`）
- 自动探测 `factorio_dir`，自动生成并持久化 Cookie 密钥、RCON 口令与端口
- 保存/模组/模组包上传下载均为流式，降低内存占用
- Mods 目录并发保护：进程内 RwLock + 跨进程 lockfile（`.fsm_mods.lock`）
- WebSocket 推送 `server_status` 与 `gamelog`，连接即回放快照
- 便捷行为：隐藏 `*.tmp.zip`；仅在 Factorio ≥ 0.17 传入 `--server-adminlist`

快速开始（本地 HTTP 调试）
1) 将官方 headless 解压到仓库根的 `./factorio`（至少包含 `bin/x64/factorio`、`data/base`、`config`）
2) 在仓库根运行：
   - `FSMR_SECURE=false RUST_LOG=info cargo run`
3) 首次启动会：
   - 探测 `factorio_dir`；在 `conf.toml` 写入 `cookie_key_b64`、`factorio_rcon_pass`、`factorio_rcon_port`
   - 初始化数据库并创建默认 `admin`（控制台打印随机密码）
4) 打开 `http://127.0.0.1:3000/login` 登录（`admin + 控制台密码`）

Docker
- 在 `docker/` 目录快速启动：
  - `docker compose -f docker/docker-compose.simple.yaml up -d`
- 首次运行会下载 Factorio headless。默认将数据与配置持久化到 `./docker/fsm-data` 与 `./docker/factorio-data/*`。

配置
- 主配置文件：`conf.toml`（示例：`conf.toml.example`）。
- 环境变量前缀为 `FSMR_`（例如 `FSMR_CONF`、`FSMR_BIND_ADDR`、`FSMR_SECURE=false`）。
- 首次运行会把生成的值（cookie 密钥、RCON 口令/端口）写回 `conf.toml`。

开发
- 构建：`cargo build`
- 运行（详细日志）：`RUST_LOG=debug,tower_http=debug,axum=debug cargo run`
- 前端打包：`npm install && npm run build` 或 `make app/bundle`（产物位于 `app/`）

许可协议
- MIT — 详见 `LICENSE.md`。同时感谢原项目与所有贡献者。

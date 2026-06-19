# 更新日志

## 2026-06-19

### 依赖升级
- `clap`: 4.6
- `colored`: 3.1
- `nix`: 0.31（从 0.27 升级）
- `ctrlc`: 3.5

### 代码优化
- 删除 `supervisor.rs` 中未使用的 `max_retries` 和 `base_backoff` 字段
- 删除重复的 `get_service_list` 函数，统一使用 `config::get_service_list`
- 添加 `colored::Colorize` 特性导入（colored 3.1 需要显式导入）

### DNS 优化
- 生成 `resolv.conf` 时追加 DNS 优化选项：
  ```
  options timeout:1 attempts:1 rotate single-request-reopen
  ```
  - `timeout:1` - DNS 查询超时 1 秒（默认 5 秒）
  - `attempts:1` - 只尝试 1 次（默认 2 次）
  - `rotate` - 轮询 DNS 服务器
  - `single-request-reopen` - 避免 IPv4/IPv6 并发查询超时

### 信号处理改进
- 提前注册信号处理器，在任何操作前确保能捕获信号
- 支持 SIGHUP 信号（OpenRC reload 命令）
- Supervisor 每秒检查一次 stop_flag，确保快速响应信号
- 支持的信号：
  - SIGINT (Ctrl+C)
  - SIGTERM (systemd/OpenRC stop)
  - SIGHUP (OpenRC reload)

### Oneshot 模式
- 新增 `start_services_oneshot()` 函数
- 主进程 fork 子进程后立即退出
- 子进程被 init/systemd 收养继续运行
- 不监控服务状态，不自动重启

### Docker 镜像修复
- Builder 镜像添加 `--default-host` 指定 musl target：
  - x86_64: `x86_64-unknown-linux-musl`
  - arm64: `aarch64-unknown-linux-musl`
- Runtime 镜像将 `cargo test` 改为 `cargo test --lib`，避免构建时因缺少 netns 权限失败

### 镜像构建
- `netns-r-builder.arm64:latest` - 1.8GB
- `netns-r-builder.x86_64:latest` - 1.77GB
- `netns-r-runtime.arm64:latest` - 78MB
- `netns-r-runtime.x86_64:latest` - 65.5MB

## 2026-06-18

### 功能测试环境
- 创建测试配置文件 `test.yaml`
- 创建功能测试脚本 `test.sh`
- 创建环境检查脚本 `quick-test.sh`
- 创建配置验证脚本 `verify-test.sh`
- 添加 Makefile 测试目标

### Supervisor 修复
- 移除 `max_retries` 限制
- 使用指数退避（最大 60 秒）持续重试
- 修复 supervisor 5 次重启后停止的问题

### 环境变量处理
- 回滚 netns 级别环境变量
- 只保留 service 级别环境变量
- 通过 `KEY=VALUE command` 方式注入

### Docker 多平台支持
- 创建 x86_64 和 arm64 的 builder/runtime 镜像
- 支持 `make build-runtime` 和 `make build-runtime-x86_64`

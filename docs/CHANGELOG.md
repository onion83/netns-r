# 更新日志

## 2026-06-19 (commit f6ec679)

### Bug 修复
- **NAT 源地址**：`configure_nat` 的 MASQUERADE `-s` 从 `ipv4_gateway` 改为 `ipv4_address`（CIDR），修复 NAT 规则匹配不到实际流量
- **veth-peer 网关 IP**：`configure_veth_device` 新增 `configure_veth_peer_gateway`，给主机端 veth-peer 配置网关 IP（veth 模式下 veth-peer 充当命名空间网关）
- **IPv6 NAT**：`configure_nat` 新增 `ip6tables MASQUERADE` + `net.ipv6.conf.all.forwarding=1`，修复 IPv6 外网不通
- **PID 复用**：supervisor 用 `Child::try_wait` 替代 `kill -0`，避免 PID 复用导致进程存活误判
- **环境变量注入**：supervisor 用 `Command::env` 替代 `KEY=VALUE command` shell 拼接，消除注入风险
- **日志路径硬编码**：`log_system::init` 接受 `log_path` 参数，使用配置文件的 `global.log.path`
- **默认路由静默失败**：路由添加失败从 `warn` 改为 `error`，避免网关配置错误被忽略
- **cleanup 错误聚合**：`cleanup_namespaces` 收集所有错误，有失败时返回聚合 `Err`

### 改进
- `manage_namespaces`/`configure_dns` 支持 `dry_run` 参数，dry-run 走单一代码路径避免漂移
- supervisor 新增 `stop_all_services`，退出时正确 kill+wait 子进程
- 日志路径 `shell_quote` 转义，防止特殊字符破坏 shell 命令
- `get_cpu_cores` 返回 `usize` 而非 `Result<usize>`（函数永不失败）
- CLI `--log-level` 改为可选，未指定时使用配置文件级别

### 配置修复
- `example-netns.yaml`：ctc IPv4/IPv6 地址从 `.241` 改为 `.243`（避免与 cmc 重复）
- `example-netns.yaml`：cuc/ctc 的 `service-list` 从 `network` 内部移到同级（修复 YAML 层级错误）
- `test.yaml`：test-ns1/ns2 补充与网段匹配的 `ipv4-gateway`/`ipv6-gateway`

### 文档
- `docs/README.md`、`CODEBUDDY.md`：移除不存在的 `service.rs` 条目
- 新增 `docs/ROADMAP.md`：性能提升和功能改进路线图

### 验证
- `cargo clippy` 零警告，7 个集成测试通过
- arm64 runtime 镜像 IPv4/IPv6 双栈网络测试通过

## 2026-06-19 (早期)

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

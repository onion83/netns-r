# CODEBUDDY.md This file provides guidance to CodeBuddy when working with code in this repository.

## 项目概述

`netns-r` 是一个 Rust 编写的 Linux 网络命名空间管理工具：用一份 YAML 配置创建网络命名空间、配置接口/地址/路由/DNS/NAT，并在命名空间内启动和监控服务进程。它通过调用系统的 `ip`、`iptables`、`ip6tables`、`sysctl` 完成所有网络操作，不使用 netlink，也没有容器镜像或文件系统隔离。

当前仍是原型，存在已知缺陷（见 `docs/known-issues.md`）。修改运行逻辑前先读该文件，不要把历史测试结果当作当前行为的验证证据。

## 常用命令

所有命令在仓库根目录执行。Makefile 只是下列 cargo 命令的包装。

```sh
cargo build --locked                      # 开发构建（make build）
cargo build --release --locked            # 发布构建（make release）
cargo check --locked                      # 快速类型检查（make check）
cargo test --locked                       # 全部测试（make test）
cargo test --locked --test integration test_variable_replacement   # 运行单个测试
cargo clippy --all-targets --locked -- -D warnings                 # 严格 lint，警告即失败（make clippy）
cargo fmt --all -- --check                # 格式检查（make fmt-check）
cargo fmt --all                           # 格式化，会改写源码（make fmt）
cargo clean                               # 清理构建产物（make clean）
```

运行二进制（需要 root 和 iproute2，只能在隔离的 Linux 环境执行）：

```sh
sudo ./target/release/netns-r --config conf/config.example.simple.yaml           # 前台运行
sudo ./target/release/netns-r --config conf/config.example.simple.yaml --clean   # 按配置清理资源
./target/release/netns-r --config conf/config.example.simple.yaml --dry-run      # 当前仍有副作用，不是安全的预检查
```

Docker 镜像（构建上下文为仓库根目录，Dockerfile 在 `docker/`）：

```sh
make build-builder PLATFORM=arm64     # 构建镜像 netns-r-builder.<PLATFORM>
make build-runtime PLATFORM=arm64     # 先构建 builder，再构建 netns-r-runtime.<PLATFORM>
                                      # PLATFORM 可取 arm64 或 x86_64
```

runtime 镜像默认进入 shell，不自动启动程序；两份示例配置位于 `/etc/netns-r/config.example.simple.yaml` 和 `/etc/netns-r/config.example.full.yaml`，运行时须显式传入 `--config`。

## 测试说明

- 全部测试都在 `tests/integration.rs`（7 个），通过 `lib.rs` 导出的模块访问代码。`src/` 内没有单元测试，`cargo test --lib` 不会运行任何测试。
- `pidfile` 只在 `main.rs` 中声明为私有模块，`lib.rs` 未导出，因此集成测试无法覆盖它。
- 配置类测试读取 `conf/config.example.simple.yaml`，该文件缺失时会跳过而非失败；完整示例 `config.example.full.yaml` 没有测试覆盖。
- 在 Linux 上，`test_environment_check` 会真实创建并删除 `test-netns-check` 命名空间，所以只能在隔离环境运行测试；在 macOS 上跳过。
- `test_log_system` 使用固定路径 `/tmp/test-netns-r.log`，不要并行运行多个测试实例。
- macOS 上编译和测试通过只说明代码可构建，不能替代 Linux 网络验证。验证步骤见 `docs/verification.md`。
- `cargo fmt --all -- --check` 目前因既有格式差异失败。除非任务要求格式化，不要运行 `cargo fmt`，否则会产生与任务无关的大范围 diff。

## 架构

### 包结构

单个 crate，同时提供库和二进制。`main.rs` 用 `mod` 引入所有模块；`lib.rs` 导出其中六个供集成测试使用。依赖很少：`serde`/`serde_yaml`（配置）、`clap`（CLI）、`nix`/`libc`（信号和进程）、`log`（日志门面）、`anyhow`（错误）。没有异步运行时，全部是同步阻塞代码。

### 启动流程（`main.rs`）

顺序固定，且部分步骤不受 `--dry-run` 保护：

1. 加载 YAML，再用配置中的日志路径初始化日志（配置必须先于日志加载）。
2. `env_check::check_environment` 检查系统信息和 netns 能力，随后 `config::validate_config` 做基础校验。
3. `--clean`：清理命名空间和 PID/状态文件后直接退出。
4. 检测上次异常退出的残留状态，如有则执行恢复清理。
5. 注册信号处理器。
6. `--dry-run`：打印网络命令和服务命令后退出。
7. 写 PID 文件和状态文件，`netns::manage_namespaces` 创建并配置网络。
8. 根据 `global.supervisord.enable` 进入监控模式（前台循环，退出后清理网络）或单次模式（启动子进程后退出，命名空间和服务保留）。
9. 移除 PID/状态文件。

### 配置模型与继承（`config.rs`）

配置分三层：`global`（supervisord 开关、检查周期、日志）、`default`（接口模式、默认出口、默认网关/DNS、默认服务列表）、`netns-list`（每个命名空间的网络和服务）。

- 接口模式由 `default.use-raw-dev` 全局决定：`raw-dev` 把已有物理/虚拟接口移入命名空间；`veth` 创建虚拟设备对，主机侧固定命名为 `<veth-dev>-peer`，并充当命名空间网关。
- 网关、出口、DNS 的规则是命名空间级优先，缺省回退到 `default`。
- 服务列表是整体覆盖：命名空间设置 `service-list` 就完全替代默认列表，`[]` 表示不启动服务。
- 布尔字段同时接受 `true/false` 和 `yes/no`。
- `$(netns)`、`$(service)` 占位符在服务命令和日志路径中替换（`replace_variables`）。
- 解析器接受 `default.log-file` 和命名空间级 `env`，但运行代码不使用它们，属于已知的配置与行为不一致。

### 命令执行的两条路径

- **`command.rs` 的 `SystemCommandExecutor`**：所有网络配置命令的唯一出口。`dry_run=true` 时只打印并返回成功的空输出，不执行。`execute_in_ns` 等价于 `ip netns exec <ns> <cmd> <args>`。注意 dry-run 下 `ip netns list` 返回空 stdout，所以 dry-run 永远不会显示"命名空间已存在"时的删除步骤。
- **`supervisor.rs` 直接使用 `std::process::Command`**：服务进程通过 `ip netns exec <ns> sh -c <command>` 启动，不经过 executor，环境变量用 `Command::env` 注入，日志路径经单引号转义后拼进 shell 命令。因此 dry-run 的服务命令由 `main.rs` 的 `show_dry_run_services` 单独打印。

`netns.rs` 中的 `configure_dns` 也不走 executor，直接写 `/etc/netns/<name>/resolv.conf`。

新增系统操作时，先确定它属于哪条路径，并同步考虑 dry-run 的表现。

### 网络资源生命周期（`netns.rs`）

`manage_namespaces` 对每个命名空间：若同名命名空间已存在则删除（raw-dev 先把接口移回 PID 1 的命名空间），然后创建、启用 loopback、配置接口、地址、默认路由、DNS。

- NAT 仅在 veth 模式且配置了 `outbound-dev` 时启用：添加 MASQUERADE 规则并用 `sysctl` 打开主机转发，要求配置 IPv4 地址。
- `cleanup_namespaces` 被 `--clean` 和监控模式退出共用，按"移回 raw-dev → SIGKILL 命名空间内进程 → 删除命名空间 → 删除 veth peer → 删除 NAT 规则 → 删除 DNS 目录"的顺序执行，收集错误后统一返回。
- 资源没有所有权标记，靠配置中的名称反推。启动失败没有统一回滚，可能残留部分资源。

### 服务监控（`supervisor.rs`）

- 服务以 `"<netns>/<service>"` 作为状态键，重启时用 `/` 拆分还原，所以命名空间名和服务名不能包含 `/`。
- `ServiceState` 是状态机：`Running` 持有 `Child` 并用 `try_wait()` 判断退出；`Failed`/`Restarting` 携带重试计数，按 `min(60, 2^retry_count)` 秒指数退避，无限重试（幂运算可能溢出，是已知问题）。
- 监控循环每秒醒来一次：检查停止标记，累计到 `check-interval` 才检查并重启服务。停止响应和服务检查不是同一周期。
- 服务环境自动附加 `NETNS_NAME`、`SERVICE_NAME`，会覆盖同名配置。

### 信号处理（`main.rs` + `supervisor.rs`）

处理器只做 `AtomicBool::store`，不做 IO 和日志。SIGINT/SIGTERM 置位 `SUPERVISOR_STOP_FLAG`，监控循环退出后由主流程清理网络。SIGHUP 置位 `RELOAD_FLAG`，循环中只打印"尚未实现重载"，不会重新读取配置。

### 崩溃恢复（`pidfile.rs`）

PID 文件记录主进程 PID，状态文件记录拟创建的命名空间名。启动时 `check_stale_pidfile` 结合 PID 是否存活、`/proc/<pid>/cmdline` 是否为 netns-r、状态文件中的命名空间是否仍存在来判断残留。这两个文件不是独占锁，当前逻辑会把正在运行的实例误判为残留并清理其资源，所以不支持重复启动。

## 运行边界

- 服务命令经 `sh -c` 以 root 执行，配置文件等同于可执行代码，只能加载可信配置。
- 程序会替换同名命名空间并杀死其中进程，NAT 模式会修改主机防火墙和转发开关且清理时不恢复。
- 不要把 `--clean` 与 `--dry-run` 组合使用，`--clean` 目前不保证幂等。

## 文档

- `docs/usage.md`：配置字段、CLI、运行和清理
- `docs/design.md`：模块职责、执行流程图、资源生命周期
- `docs/verification.md`：本机检查和隔离 Linux 验证步骤
- `docs/known-issues.md`：发布前待修复问题清单
- `docs/history.md`：行为变化记录
- `conf/config.example.simple.yaml`：最小 veth 示例（一个命名空间、一个 `sleep` 服务、无 NAT）
- `conf/config.example.full.yaml`：带注释的完整示例（raw-dev 模式、三个命名空间、默认网关/DNS/服务列表），设备名和地址需按实际环境修改
- `conf/config.yaml`：本地配置，已被 Git 忽略

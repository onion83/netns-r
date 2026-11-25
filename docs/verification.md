# 开发与验证

## 本机检查

从仓库根目录执行：

```sh
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo build --release --locked
```

Makefile 提供 `build`、`release`、`test`、`check`、`clippy`、`fmt-check` 和 `help`。`fmt` 会修改源码，其他检查不会主动重排源码。

现有 7 项集成测试覆盖示例配置读取、基础验证、模板变量、服务列表、命令执行、日志及环境检查入口。配置测试在示例不存在时会跳过；环境测试在非 Linux 平台跳过，在 Linux 检查返回错误时也不一定失败。绿色结果不等于完成网络或故障恢复验证。

Linux 下环境检查测试会尝试创建、删除 `test-netns-check`，因此只在隔离环境运行测试。测试日志使用固定 `/tmp/test-netns-r.log`，避免同时运行多个测试实例。

## 隔离 Linux 验证

使用专用虚拟机或可丢弃的 Linux 容器，安装 iproute2 和所需网络工具。不要用生产命名空间或承载管理流量的网卡。

使用仓库中的示例，而非本地生产配置：

```sh
sudo ./target/release/netns-r --config conf/config.example.simple.yaml
```

该命令保持前台运行。在另一个终端检查：

```sh
sudo ip netns list
sudo ip netns exec demo-ns ip addr show
sudo ip netns exec demo-ns ip route show
sudo ip netns exec demo-ns ping -c 3 10.203.0.1
sudo ip netns pids demo-ns
```

示例只验证命名空间到主机侧 veth 的连通性，不验证公网、NAT 或 DNS。主终端 Ctrl+C 后检查命名空间与 `veth-demo-peer` 是否删除。若异常退出，先确认监控进程已停止，再按同一配置执行 `--clean`，并人工检查残留资源。

发布前还需要独立验证：双栈 NAT/DNS、raw-dev 恢复、子进程重启、启动失败回滚、并发实例拒绝和 dry-run 无副作用。其中若干项目当前仍有已知缺陷，不能标为已通过。

## Docker 构建

两种架构的 builder/runtime Dockerfile 统一放在 `docker/`。构建上下文仍为项目根目录，`.dockerignore` 保留在根目录，Dockerfile 内的 `COPY` 源路径不变：

```sh
make build-builder PLATFORM=arm64
make build-runtime PLATFORM=arm64
# x86_64 平台使用 PLATFORM=x86_64
```

构建 runtime 会先构建对应 builder。跨架构构建依赖 Docker 环境的模拟能力；镜像构建成功不代表 Linux 特权网络验证通过。

runtime 默认进入 shell，不自动启动网络管理器；两份示例配置位于 `/etc/netns-r/config.example.simple.yaml` 和 `/etc/netns-r/config.example.full.yaml`。运行时应显式传入 `--config`。需要网络命名空间权限的容器运行由测试者在隔离环境配置，普通镜像构建阶段不执行此类检查。

旧 shell 测试脚本已移除：它们存在路径失效、前台 supervisor 阻塞后续检查、管道掩盖失败等问题。原文件保留在 `prototype` 分支，不能作为当前自动化测试入口。

## 本轮验证记录

2026-11-25 整理前，本机 macOS 的 7 项集成测试和严格 Clippy 通过；格式检查未通过，Makefile 存在语法错误。本轮只整理文档、配置入口和构建辅助文件，不修复 Rust 运行逻辑。旧报告里的镜像大小、性能和网络测试结论不作为当前版本的验证证据。

整理后复核：7 项集成测试、严格 Clippy、macOS Release 构建、CLI 帮助、文档链接和 `git diff --check` 均通过。Makefile 帮助与 Docker 命令展开正常。格式检查仍因既有 Rust 格式差异失败，本轮没有格式化源码。未执行 Docker 镜像构建或 Linux 特权网络测试。

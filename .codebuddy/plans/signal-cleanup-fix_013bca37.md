---
name: signal-cleanup-fix
overview: 修复 netns-r 父进程被杀后子进程和网络环境残留的问题。核心改动：(1) 服务进程用 process_group(0) 启动 + 按进程组 kill 解决孙子进程泄漏；(2) 信号处理器只设 atomic flag 不做 IO，消除 UB；(3) SIGHUP 改为 reload 语义不再当 stop；(4) 写 PID 文件 + 状态文件支持 OpenRC 集成和 SIGKILL 后恢复；(5) 启动时检测残留状态自动清理。涉及 main.rs、supervisor.rs、netns.rs，新增 src/pidfile.rs。
---

## 用户需求

父进程被 kill 或 OpenRC 发信号后，子进程和网络环境没有清理干净。需要修复信号处理和进程清理机制。

## 问题分析

当前代码存在四个关键缺陷：

1. **孙子进程泄漏**：`ip netns exec ns sh -c "cmd"` 产生三层进程树 `ip → sh → 服务进程`。`stop_all_services()` 只 `child.kill()` 杀直接子进程 `ip`，`sh` 和服务进程变孤儿继续运行。
2. **SIGKILL 无 grace period**：`stop_all_services()` 直接 `child.kill()`（SIGKILL），服务进程无机会清理资源。
3. **信号处理器 UB**：`handle_sighup` 在 signal handler 上下文调用 `warn!()`，async-signal-unsafe，是未定义行为。
4. **SIGHUP 误当 stop**：OpenRC reload 信号被当成停止信号，导致 `rc-service reload` 杀死服务。
5. **无崩溃恢复**：SIGKILL 后 `cleanup_namespaces` 不执行，命名空间/veth-peer/iptables 规则/DNS 目录全部残留。

## 核心功能

- 使用进程组（process_group）启动服务，确保 SIGTERM/SIGKILL 能到达所有子孙进程
- 优雅停止：先 SIGTERM → 等待 grace period → 未退出则 SIGKILL 整个进程组
- 信号处理器改为纯 atomic flag 操作，不做任何 IO
- SIGHUP 独立为 reload flag，不再触发停止
- PID 文件 + 状态文件：启动时检测上次异常退出并恢复清理，正常运行期间记录状态，正常退出时删除
- `--pidfile` CLI 参数支持 OpenRC 集成

## Tech Stack

- 现有技术栈：Rust + serde_yaml + clap + nix + iproute2 命令封装
- 移除依赖：`ctrlc`（替换为 nix sigaction 统一处理所有信号）
- 新增依赖：无（nix 已有 signal+process features，serde_yaml 用于状态文件序列化）
- `std::os::unix::process::CommandExt::process_group(0)` 来自标准库，无需额外依赖

## Implementation Approach

### 1. 进程组管理（核心修复）

**问题**：`Command::new("ip").args(["netns","exec",ns,"sh","-c",cmd]).spawn()` 产生 `ip(PGID=A) → sh(PGID=A) → service(PGID=A)`，但 `stop_all_services` 只 `child.kill()` 杀 `ip`，`sh` 和 `service` 变孤儿。

**方案**：在 `start_service_process` 中添加 `cmd.process_group(0)`，使子进程成为新进程组 leader（PGID = child PID）。`ip netns exec` 内部 fork 不改 PGID，因此 `ip`、`sh`、`service` 三者共享同一 PGID。停止时用 `nix::sys::signal::kill(Pid::from_raw(-pid), Signal::SIGTERM)` 向整个进程组发信号。

**关键决策**：不复用 `configure_raw_device` 语义（那个移整个设备入 ns），VLAN 父设备必须留主机。进程组 kill 用负 PID 语义，nix `Pid::from_raw(-pid)` 即可。

### 2. 优雅停止（SIGTERM → grace → SIGKILL）

**方案**：`stop_all_services` 重写为三阶段：

- Phase 1：向所有运行中服务的进程组发 SIGTERM，记录 PID 列表
- Phase 2：轮询 `try_wait`，10 秒 grace period 内全部退出则 break
- Phase 3：超时未退出的进程组发 SIGKILL，最后 `wait()` 回收所有僵尸

### 3. 信号处理器修复

**方案**：移除 `ctrlc` 依赖，统一用 `nix::sys::signal::sigaction` 注册 SIGINT/SIGTERM/SIGHUP。handler 函数只做 `AtomicBool::store(true, Relaxed)`，不做任何 IO/logging。日志输出移到 supervisor 循环检测 flag 后执行。

- `STOP_FLAG: AtomicBool`：SIGINT/SIGTERM 设置 → supervisor 退出 → cleanup
- `RELOAD_FLAG: AtomicBool`：SIGHUP 设置 → supervisor 循环检测后 log + continue（完整 reload 是 ROADMAP）
- 移除 `Arc
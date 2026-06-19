# netns-r 改进路线图

> 状态：暂存，未开始实施。最后更新：2026-06-19

## 性能提升

### 1. Supervisor 轮询改事件驱动（P0）
- **现状**：`supervisor.rs` 用 `std::thread::sleep(1s)` + 轮询 `try_wait` 检查服务存活，每秒唤醒，服务多时浪费 CPU 且检测延迟最高 1 秒
- **方案**：每个服务用独立线程 `child.wait()` 阻塞等待退出，通过 `mpsc::channel` 向主循环发送退出事件。主循环用 `recv_timeout(check_interval)` 同时处理退出事件和 stop_flag 检查
- **收益**：零轮询，进程退出立即感知，CPU 占用趋近于 0
- **复杂度**：中等（线程安全 + channel 生命周期）

### 2. 命令执行器批量优化（P1）
- **现状**：每个 `ip`/`iptables`/`sysctl` 命令都 fork+exec 一次，创建一个命名空间要执行 10+ 条命令
- **方案**：对无依赖的命令（如启用 IPv4 转发 + IPv6 转发、添加 IPv4 NAT + IPv6 NAT）合并为单次 `sh -c` 调用
- **收益**：命名空间创建时间减少 30-50%
- **复杂度**：低

### 3. iptables 改 nftables 或 ipset（P2）
- **现状**：每个命名空间添加独立的 MASQUERADE 规则，规则数随命名空间线性增长
- **方案**：用 `ipset` 创建源地址集合，单条规则匹配整个集合；或迁移到 nftables（原生支持集合）
- **收益**：规则数从 O(n) 降为 O(1)，大型部署下防火墙性能更优
- **复杂度**：中

## 功能改进

### 4. 健康检查（P0）
- **现状**：supervisor 只能检测进程退出，无法检测"进程活着但服务不可用"（如 HTTP 服务死锁）
- **方案**：`ServiceConfig` 增加 `health-check` 配置：
  ```yaml
  health-check:
    type: tcp          # tcp | http | command
    target: ":8080"    # 端口或 URL
    interval: 10       # 秒
    timeout: 3
    retries: 3         # 连续失败次数触发重启
  ```
  supervisor 周期性探活，连续失败则 kill+restart
- **复杂度**：中

### 5. 优雅停止（P0）
- **现状**：`stop_all_services` 直接 `child.kill()`（SIGKILL），服务没有机会清理资源
- **方案**：先 `SIGTERM` → 等待 grace period（默认 10 秒，可配置 `stop-grace-period`）→ 仍未退出则 `SIGKILL`
- **复杂度**：低

### 6. 服务依赖编排（P1）
- **现状**：所有服务并行启动，无法表达"A 必须在 B 之前启动"
- **方案**：`ServiceConfig` 增加 `after: [db]` 依赖声明，supervisor 拓扑排序后按序启动
- **复杂度**：中

### 7. 配置热重载（P1）
- **现状**：SIGHUP 只触发停止，不支持 reload
- **方案**：SIGHUP 触发重新加载 `netns.yaml`，diff 出新增/删除/变更的命名空间和服务，仅对变更部分执行 create/delete/restart
- **复杂度**：中

### 8. Dry-run 增强（P2）
- **现状**：dry-run 输出散乱，不易程序化分析
- **方案**：dry-run 模式输出结构化执行计划（JSON），列出每个命名空间将执行的所有命令
- **复杂度**：低

### 9. Prometheus metrics 暴露（P2）
- **方案**：supervisor 启动轻量 HTTP server（`0.0.0.0:9090/metrics`），暴露：
  - `netns_service_up{name, namespace}` — 服务存活状态
  - `netns_service_restart_count_total` — 重启次数
  - `netns_service_uptime_seconds` — 运行时长
- **复杂度**：低

### 10. 状态持久化（P3）
- **现状**：supervisor 状态在内存中，进程重启后丢失重启计数
- **方案**：将服务状态持久化到 SQLite 或 JSON 文件，重启后恢复
- **复杂度**：中

## 优先级矩阵

| 方案 | 价值 | 复杂度 | 优先级 |
|------|------|--------|--------|
| 1. Supervisor 事件驱动 | 高 | 中 | P0 |
| 4. 健康检查 | 高 | 中 | P0 |
| 5. 优雅停止 | 高 | 低 | P0 |
| 7. 配置热重载 | 中 | 中 | P1 |
| 6. 服务依赖编排 | 中 | 中 | P1 |
| 2. 命令批量执行 | 中 | 低 | P1 |
| 8. Dry-run JSON 输出 | 中 | 低 | P2 |
| 9. Prometheus metrics | 中 | 低 | P2 |
| 3. nftables/ipset | 低 | 中 | P2 |
| 10. 状态持久化 | 低 | 中 | P3 |

## 实施建议

建议从 P0 三项入手（事件驱动、健康检查、优雅停止），它们对生产可用性影响最大，且互相独立可并行开发。

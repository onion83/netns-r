# netns-r

一个用 Rust 编写的网络命名空间管理工具，用于在 Linux 系统中管理多个网络命名空间，特别适用于多 ISP（互联网服务提供商）场景。

## 功能特性

- **配置驱动**: 使用 YAML 配置文件定义网络命名空间
- **变量替换**: 支持 `$(netns)` 和 `$(service)` 变量
- **多网络模式**: 支持原始设备和 veth 设备两种模式
- **服务管理**: 在命名空间内启动和管理服务
- **进程监控**: 支持进程守护和自动重启（指数退避，最大 60 秒）
- **日志系统**: 三级日志（debug、info、error），彩色输出
- **dry-run 模式**: 显示将要执行的命令而不实际执行
- **信号处理**: 支持 SIGINT、SIGTERM、SIGHUP 信号，确保优雅关闭
- **Oneshot 模式**: fork 子进程后主进程退出，适合由 init/systemd 管理
- **DNS 优化**: 自动添加 `timeout:1 attempts:1 rotate single-request-reopen` 选项

## 快速开始

### 安装

```bash
# 克隆项目
git clone <repository-url>
cd netns-r

# 编译项目
cargo build --release

# 安装到系统
make install
```

### 使用

```bash
# 运行（需要读取配置文件）
./target/release/netns-r --config netns.yaml

# dry-run 模式（显示将要执行的命令）
./target/release/netns-r --config netns.yaml --dry-run

# 调试模式
./target/release/netns-r --config netns.yaml --log-level debug
```

### 配置文件

创建 `netns.yaml` 配置文件：

```yaml
# 全局配置
global:
  supervisord:
    enable: no
    check-interval: 60
  log:
    level: debug
    path: "/tmp/netns-r.log"

# 默认配置
default:
  use-raw-dev: no
  network:
    ipv4-gateway: "10.0.0.1"
    ipv6-gateway: "fd00::1"
  log-file:
    path: "/tmp/netns-$(netns)-$(service).log"
  service-list:
  - name: echo-test
    command: "echo 'Hello from $(netns) namespace'"

# 网络命名空间列表
netns-list:
- name: test-ns1
  network:
    veth-dev: "veth-test1"
    ipv4-address: "10.0.1.2/24"
    ipv4-gateway: "10.0.1.1"
    dns-servers:
    - 8.8.8.8
```

## 测试

### 单元测试

```bash
# 运行单元测试
cargo test

# 通过 Makefile 运行
make test
```

### 容器测试

使用 Docker 容器进行真实环境测试（推荐）：

```bash
# 运行完整容器测试
./container-test.sh test

# 运行 dry-run 测试
./container-test.sh dry-run

# 构建 Docker 镜像
./container-test.sh build

# 清理容器资源
./container-test.sh clean

# 通过 Makefile 运行
make test-container
make test-container-dry-run
make test-container-clean
```

容器测试优势：
- 在真实 Linux 环境中测试
- 无需 root 权限（Docker 自动处理）
- 隔离的测试环境
- 自动清理资源

### 功能测试

功能测试需要在 Linux 环境中以 root 权限运行：

```bash
# 验证测试配置
make test-verify

# 检查测试环境
make test-check

# 运行功能测试
sudo make test-functional

# 查看测试状态
make test-status

# 清理测试环境
sudo make test-clean
```

## 项目结构

```
netns-r/
├── Cargo.toml          # Rust 项目配置
├── Makefile            # 构建和测试管理
├── Dockerfile.*        # Docker 镜像构建（x86_64/arm64）
├── src/
│   ├── main.rs         # 主程序入口
│   ├── config.rs       # 配置解析模块
│   ├── netns.rs        # 网络命名空间管理
│   ├── command.rs      # 命令执行器
│   ├── service.rs      # 服务管理
│   ├── supervisor.rs   # 进程守护器
│   ├── log_system.rs   # 日志系统
│   └── env_check.rs    # 环境检查
├── tests/
│   └── integration.rs  # 集成测试
├── test.yaml           # 功能测试配置文件
├── test.sh             # 功能测试脚本
├── quick-test.sh       # 环境检查脚本
├── verify-test.sh      # 配置验证脚本
└── docs/
    ├── README.md       # 项目文档（本文件）
    ├── README-TEST.md  # 测试说明文档
    ├── SUMMARY.md      # 项目总结文档
    ├── CHANGELOG.md    # 更新日志
    └── FINAL-REPORT.md # 完整的工作报告
```

## 文档

- [项目文档](CODEBUDDY.md) - 项目概述和使用指南
- [测试说明](README-TEST.md) - 功能测试详细说明
- [项目总结](SUMMARY.md) - 项目开发总结
- [更新日志](CHANGELOG.md) - 版本更新记录
- [完整报告](FINAL-REPORT.md) - 完整的工作报告

## 开发

### 依赖

- Rust 工具链
- serde, serde_yaml - 配置解析
- clap 4.6 - 命令行参数解析
- colored 3.1 - 终端颜色输出
- chrono 0.4 - 时间处理
- anyhow 1.0 - 错误处理
- log 0.4 - 日志接口
- nix 0.31, libc 0.2 - Unix 系统调用
- ctrlc 3.5 - 信号处理

### 构建

```bash
# 开发构建
cargo build

# 发布构建
cargo build --release

# 代码检查
cargo clippy

# 格式化代码
cargo fmt
```

### 测试

```bash
# 单元测试
cargo test

# 功能测试（需要 root 权限）
sudo make test-functional
```

## 环境要求

### 系统要求
- Linux 操作系统（支持网络命名空间）
- root 权限（创建命名空间需要）
- iproute2 工具包
- iptables（用于 NAT 配置）

### 开发环境
- Rust 工具链
- 项目依赖（通过 Cargo.toml 管理）

## 使用场景

### 1. 多 ISP 网络隔离
为不同的互联网服务提供商创建独立的网络命名空间，实现网络流量隔离。

### 2. 网络测试环境
创建隔离的网络环境进行网络应用测试。

### 3. 容器网络管理
为容器化应用提供网络命名空间管理。

### 4. 网络服务部署
在不同的网络命名空间中部署网络服务，实现服务隔离。

## 示例

### 多 ISP 配置示例

```yaml
global:
  supervisord:
    enable: yes
    check-interval: 60
  log:
    level: info
    path: "/var/log/netns-r.log"

default:
  use-raw-dev: no
  network:
    ipv4-gateway: "192.168.1.1"
  log-file:
    path: "/var/log/netns-$(netns)-$(service).log"
  service-list:
  - name: dping-server
    command: "/usr/bin/dping-server"
    log-file:
      path: "/var/log/netns-$(netns)-$(service).log"

netns-list:
- name: cmc  # 中国移动
  network:
    raw-dev: "eth-cmc"
    veth-dev: "veth-cmc"
    ipv4-address: "192.168.11.241/24"
    ipv4-gateway: "192.168.11.1"
    dns-servers:
    - 211.138.240.100
    - 211.138.245.180

- name: cuc  # 中国联通
  network:
    raw-dev: "eth-cuc"
    veth-dev: "veth-cuc"
    ipv4-address: "192.168.11.242/24"
    ipv4-gateway: "192.168.11.1"
    dns-servers:
    - 221.7.128.68
    - 221.7.136.68

- name: ctc  # 中国电信
  network:
    raw-dev: "eth-ctc"
    veth-dev: "veth-ctc"
    ipv4-address: "192.168.11.243/24"
    ipv4-gateway: "192.168.11.1"
    dns-servers:
    - 202.103.224.68
    - 202.103.225.68
```

## 故障排除

### 常见问题

1. **权限不足**
   ```
   错误: 创建网络命名空间需要 root 权限
   解决: 使用 sudo 运行程序
   ```

2. **命名空间已存在**
   ```
   错误: 命名空间已存在
   解决: 先删除现有命名空间或使用不同的名称
   ```

3. **网络配置失败**
   ```
   错误: 无法配置 IP 地址
   解决: 检查网络配置是否正确，确保子网不冲突
   ```

### 调试技巧

1. **查看详细日志**
   ```bash
   tail -f /tmp/netns-r.log
   ```

2. **手动测试命名空间**
   ```bash
   sudo ip netns exec test-ns1 bash
   ip addr show
   ping 10.0.1.1
   ```

3. **检查系统状态**
   ```bash
   ip netns list
   ip link show
   ip route show
   ```

## 贡献

欢迎贡献代码、报告问题或提出改进建议。

## 许可证

MIT License
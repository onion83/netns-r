# CODEBUDDY.md This file provides guidance to CodeBuddy when working with code in this repository.

## 项目概述

`netns-r` 是一个用 Rust 编写的**网络命名空间管理工具**，用于在 Linux 系统中管理多个网络命名空间（network namespace），特别适用于多 ISP（互联网服务提供商）场景。

项目当前处于初始阶段，主要包含配置文件 `netns.yaml`。

## 配置文件结构

`netns.yaml` 是核心配置文件，采用 YAML 格式，包含三个主要部分：

### 1. 全局配置 (`global`)
- `supervisord`: 进程守护配置（是否启用、检查间隔）
- `log`: 日志配置（级别、路径）

### 2. 默认配置 (`default`)
- `use-raw-dev`: 是否使用原始网络设备
- `network`: 默认网络配置（IPv4/IPv6 网关）
- `log-file`: 日志文件路径模板（支持变量 `$(netns)` 和 `$(service)`）
- `service-list`: 默认服务列表，每个服务包含：
  - `name`: 服务名称
  - `command`: 完整命令和参数
  - `log-file`: 日志文件路径（可选）
  - `env`: 环境变量（可选）

### 3. 网络命名空间列表 (`netns-list`)
定义多个网络命名空间容器，每个包含：
- `name`: 命名空间名称（如 `cmc`, `cuc`, `ctc`）
- `network`: 网络配置
  - `raw-dev`: 原始网络设备名称
  - `veth-dev`: veth 设备名称
  - `ipv4-address` / `ipv6-address`: IP 地址
  - `ipv4-gatway` / `ipv6-gatway`: 网关（注意：配置中拼写是 `gatway` 而非 `gateway`）
- `env`: 命名空间级别的环境变量（可选），会自动注入到该命名空间的所有服务
  - 如果服务级别有同名环境变量，服务级别的值会覆盖命名空间级别的值
  - 标准环境变量 `NETNS_NAME` 和 `SERVICE_NAME` 会自动添加
  - `dns-servers`: DNS 服务器列表
- `service-list`: 该命名空间的服务列表（覆盖默认配置）

## 当前配置的 ISP 场景

配置中定义了三个网络命名空间，对应中国三大运营商：
- **cmc**: 中国移动 (China Mobile)
- **cuc**: 中国联通 (China Unicom)
- **ctc**: 中国电信 (China Telecom)

## 开发说明

### 构建项目
```bash
# 初始化 Rust 项目（如果尚未初始化）
cargo init

# 构建
cargo build

# 发布构建
cargo build --release
```

### 运行
```bash
# 运行（需要读取 netns.yaml）
cargo run -- --config netns.yaml

# 或直接运行二进制文件
./target/release/netns-r --config netns.yaml
```

### 代码检查
```bash
# Clippy 代码检查
cargo clippy

# 格式化代码
cargo fmt

# 运行单元测试
cargo test

# 运行功能测试（需要 root 权限）
sudo make test-functional

# 清理测试环境
sudo make test-clean

# 查看测试状态
make test-status

# 快速检查测试环境
make test-check
```

## 技术注意事项

1. **拼写问题**: 配置文件中网关的键名拼写为 `gatway` 而非 `gateway`，解析时需注意
2. **变量替换**: 日志文件路径支持 `$(netns)` 和 `$(service)` 变量替换
3. **网络设备**: 支持两种模式 - 使用原始设备或直接创建 veth 设备对
4. **DNS 配置**: 程序需要改写 `/etc/netns/$ns/resolv.conf`
5. **进程管理**: 支持通过 supervisord 进行进程守护

## 功能测试

### 测试配置
功能测试使用 `test.yaml` 配置文件，包含三个测试命名空间：
- **test-ns1**: 10.0.1.0/24 子网，IPv4/IPv6 支持
- **test-ns2**: 10.0.2.0/24 子网，IPv4/IPv6 支持  
- **test-ns3**: 10.0.3.0/24 子网，仅 IPv4 支持

### 测试内容
- 配置解析和验证
- 网络命名空间创建和管理
- veth 设备配置和 IP 地址分配
- DNS 配置和路由设置
- 服务启动和日志管理
- 进程监控和自动重启

### 测试环境要求
- Linux 操作系统（支持网络命名空间）
- root 权限（创建命名空间需要）
- iproute2 和 iptables 工具包
- Rust 工具链

### 测试命令
```bash
# 快速检查测试环境
make test-check

# 运行单元测试
make test

# 运行功能测试（需要 root 权限）
sudo make test-functional

# 清理测试环境
sudo make test-clean

# 查看测试状态
make test-status
```

### 测试结果验证
测试脚本会自动验证：
- 命名空间创建成功
- 网络接口配置正确
- 日志文件生成正常
- 命令执行功能正常

## 项目结构

```
netns-r/
├── Cargo.toml          # Rust 项目配置
├── Cargo.lock          # 依赖锁定文件
├── Makefile            # 构建和测试管理
├── Dockerfile          # Docker 镜像构建
├── src/
│   ├── main.rs         # 主程序入口
│   ├── config.rs       # 配置解析模块
│   ├── netns.rs        # 网络命名空间管理
│   ├── command.rs      # 命令执行器
│   ├── supervisor.rs   # 进程守护器
│   ├── log_system.rs   # 日志系统
│   └── env_check.rs    # 环境检查
├── tests/
│   └── integration.rs  # 集成测试
├── example-netns.yaml  # 示例配置文件
├── test.yaml           # 功能测试配置文件
├── test.sh             # 功能测试脚本
├── quick-test.sh       # 环境检查脚本
├── README-TEST.md      # 测试说明文档
├── SUMMARY.md          # 项目总结文档
├── CODEBUDDY.md        # 本文件
└── target/             # 编译输出目录
```

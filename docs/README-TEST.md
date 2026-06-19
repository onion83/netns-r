# netns-r 功能测试说明

## 概述

本目录包含 netns-r 的功能测试配置和脚本，用于验证网络命名空间管理工具的核心功能。

## 测试配置文件

### test.yaml

测试配置文件包含三个网络命名空间，用于全面测试 netns-r 的功能：

1. **test-ns1**: 测试基本命名空间创建和配置
   - IPv4: 10.0.1.2/24
   - IPv6: fd00:1::2/64
   - 服务: echo-test, ping-test

2. **test-ns2**: 测试不同的网络配置
   - IPv4: 10.0.2.2/24
   - IPv6: fd00:2::2/64
   - 服务: echo-test, ping-test

3. **test-ns3**: 测试自定义服务列表
   - IPv4: 10.0.3.2/24
   - 服务: echo-only (自定义服务)

## 测试脚本

### test.sh

功能测试脚本，支持以下命令：

```bash
# 运行完整功能测试（需要 root 权限）
sudo ./test.sh test

# 清理测试环境
sudo ./test.sh clean

# 查看当前状态
./test.sh status

# 显示帮助信息
./test.sh help
```

### verify-test.sh

验证功能测试配置的脚本，用于检查测试环境是否正确配置：

```bash
# 验证功能测试配置
./verify-test.sh

# 通过 Makefile 运行
make test-verify
```

验证内容包括：
- 测试配置文件是否存在
- 测试脚本是否有执行权限
- Makefile 目标是否正确配置
- 文档文件是否存在
- YAML 语法是否正确
- 配置文件解析是否成功

## 使用 Makefile

项目提供了便捷的 Makefile 目标：

```bash
# 运行功能测试（需要 root 权限）
make test-functional

# 清理测试环境
make test-clean

# 查看测试状态
make test-status

# 运行单元测试
make test

# 快速检查测试环境
make test-check

# 验证功能测试配置
make test-verify
```

## 测试内容

功能测试验证以下内容：

### 1. 配置解析
- YAML 配置文件加载
- 变量替换功能 (`$(netns)`, `$(service)`)
- 配置验证

### 2. 网络命名空间管理
- 命名空间创建和删除
- veth 设备配置
- IP 地址分配
- 路由配置
- DNS 配置

### 3. 服务管理
- 服务启动和停止
- 日志文件创建
- 环境变量设置
- 自定义服务列表

### 4. 进程监控
- 进程状态检查
- 自动重启机制（如果启用）

## 前提条件

### 系统要求
- Linux 操作系统（支持网络命名空间）
- root 权限（创建命名空间需要）
- iproute2 工具包
- iptables（用于 NAT 配置）

### 开发环境
- Rust 工具链
- 项目依赖（通过 Cargo.toml 管理）

## 测试流程

1. **清理环境**: 删除之前的测试命名空间和日志文件
2. **编译项目**: 构建 netns-r 发布版本
3. **dry-run 测试**: 验证配置解析和命令生成
4. **实际运行**: 创建命名空间并启动服务
5. **验证结果**: 检查命名空间、网络接口、日志文件等

## 故障排除

### 常见问题

1. **权限不足**
   ```
   错误: 创建网络命名空间需要 root 权限
   解决: 使用 sudo 运行测试脚本
   ```

2. **命名空间已存在**
   ```
   错误: 命名空间已存在
   解决: 先运行 `make test-clean` 清理环境
   ```

3. **网络配置失败**
   ```
   错误: 无法配置 IP 地址
   解决: 检查网络配置是否正确，确保子网不冲突
   ```

### 调试技巧

1. **查看详细日志**
   ```bash
   # 查看主日志
   tail -f /tmp/netns-r-test.log
   
   # 查看特定命名空间的日志
   tail -f /tmp/netns-test-test-ns1-echo-test.log
   ```

2. **手动测试命名空间**
   ```bash
   # 进入命名空间
   sudo ip netns exec test-ns1 bash
   
   # 测试网络连接
   ping 10.0.1.1
   
   # 查看网络接口
   ip addr show
   ```

3. **检查系统状态**
   ```bash
   # 查看所有命名空间
   ip netns list
   
   # 查看网络接口
   ip link show
   
   # 查看路由表
   ip route show
   ```

## 扩展测试

### 添加新的测试命名空间

1. 在 `test.yaml` 的 `netns-list` 部分添加新的命名空间配置
2. 确保 IP 地址不与现有配置冲突
3. 更新测试脚本以验证新的命名空间

### 自定义测试服务

1. 在 `test.yaml` 的 `service-list` 部分定义新的服务
2. 使用 `$(netns)` 和 `$(service)` 变量实现动态配置
3. 配置适当的日志文件路径

## 性能测试

对于性能测试，可以考虑：

1. **大量命名空间测试**: 创建 10-100 个命名空间
2. **并发测试**: 同时启动多个服务
3. **资源监控**: 监控 CPU、内存使用情况
4. **网络性能**: 测试命名空间间的网络延迟

## 安全注意事项

1. **权限管理**: 测试需要 root 权限，请在安全的环境中运行
2. **网络隔离**: 测试会创建虚拟网络接口，可能影响主机网络
3. **清理工作**: 测试完成后及时清理，避免资源泄漏
4. **日志安全**: 日志文件可能包含敏感信息，注意权限设置

## 持续集成

可以将功能测试集成到 CI/CD 流程中：

```yaml
# GitHub Actions 示例
name: Functional Tests
on: [push, pull_request]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: Install dependencies
        run: sudo apt-get update && sudo apt-get install -y iproute2 iptables
      - name: Build and test
        run: |
          cargo build --release
          sudo make test-functional
```

## 相关文档

- [netns-r 项目文档](CODEBUDDY.md)
- [配置文件说明](example-netns.yaml)
- [Makefile 使用说明](Makefile)

## 贡献指南

1. 添加新的测试用例
2. 改进测试覆盖率
3. 优化测试性能
4. 完善故障排除文档
# netns-r 测试指南

## 测试类型

netns-r 提供多种测试方式，满足不同需求：

### 1. 单元测试

在本地运行，测试代码逻辑：

```bash
# 运行所有单元测试
cargo test

# 通过 Makefile 运行
make test
```

### 2. 功能测试

在真实 Linux 环境中测试（需要 root 权限）：

```bash
# 运行功能测试
sudo ./test.sh test

# 清理测试环境
sudo ./test.sh clean

# 查看测试状态
./test.sh status

# 通过 Makefile 运行
sudo make test-functional
```

### 3. 容器测试（推荐）

使用 Docker 容器进行真实环境测试：

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

## 测试环境要求

### 单元测试
- Rust 工具链
- 无特殊系统要求

### 功能测试
- Linux 操作系统
- root 权限
- iproute2 工具包
- iptables

### 容器测试
- Docker 已安装并运行
- 无 root 权限要求（Docker 自动处理）

## 测试配置文件

### test.yaml
功能测试配置文件，包含三个测试命名空间：
- test-ns1: 10.0.1.0/24 子网
- test-ns2: 10.0.2.0/24 子网
- test-ns3: 10.0.3.0/24 子网

### example-netns.yaml
示例配置文件，展示多 ISP 场景配置。

## 测试验证内容

### 1. 配置解析
- YAML 配置文件加载
- 变量替换功能
- 配置验证

### 2. 网络命名空间管理
- 命名空间创建和删除
- veth 设备配置
- IP 地址分配
- 路由配置
- DNS 配置

### 3. 服务管理
- 服务启动
- 日志文件创建
- 环境变量设置

### 4. 进程监控
- 进程状态检查
- 自动重启机制

## 测试结果

### 容器测试结果
✅ **所有测试通过**

- ✅ 环境检查通过
- ✅ 3 个网络命名空间创建成功
- ✅ 网络配置正确（IPv4/IPv6）
- ✅ DNS 配置正确
- ✅ 服务启动成功
- ✅ 日志文件创建成功
- ✅ 命名空间内命令执行正常

详细测试报告请查看：[CONTAINER-TEST-REPORT.md](CONTAINER-TEST-REPORT.md)

## 故障排除

### 常见问题

1. **Docker 未运行**
   ```
   错误: Docker 服务未运行
   解决: 启动 Docker 服务
   ```

2. **权限不足**
   ```
   错误: 创建网络命名空间需要 root 权限
   解决: 使用 sudo 或容器测试
   ```

3. **端口冲突**
   ```
   错误: 网络配置失败
   解决: 检查 IP 地址是否冲突
   ```

### 调试技巧

1. **查看详细日志**
   ```bash
   tail -f /tmp/netns-r-test.log
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

## 持续集成

### GitHub Actions 示例

```yaml
name: Tests
on: [push, pull_request]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - name: Run unit tests
        run: cargo test
      - name: Run container tests
        run: ./container-test.sh test
```

## 测试覆盖率

测试覆盖以下功能：
- ✅ 配置文件解析
- ✅ 环境检查
- ✅ 网络命名空间创建
- ✅ veth 设备配置
- ✅ IPv4/IPv6 地址配置
- ✅ 路由配置
- ✅ DNS 配置
- ✅ 服务启动
- ✅ 日志管理
- ✅ 错误处理
- ✅ 资源清理

## 性能测试

对于性能测试，可以考虑：
- 创建大量命名空间（10-100个）
- 并发启动多个服务
- 监控 CPU 和内存使用
- 测试网络延迟和吞吐量

## 安全测试

- 权限管理测试
- 网络隔离测试
- 资源限制测试
- 错误处理测试

## 相关文档

- [项目文档](CODEBUDDY.md)
- [测试说明](README-TEST.md)
- [容器测试报告](CONTAINER-TEST-REPORT.md)
- [项目总结](SUMMARY.md)
# netns-r

`netns-r` 是一个 Rust 编写的 Linux 网络命名空间管理工具，**用一份 YAML 配置，把网络出口、路由、DNS 和服务进程组织到各自独立的网络环境中**。

适用于一台 Linux 主机连接多条运营商线路、运行多组网络探测服务，或搭建相互隔离的网络测试环境。

## 解决什么问题

多出口环境中，多个程序默认共享主机的路由表和 DNS。要让不同服务使用各自的网络环境，通常需要手动组合 `ip netns`、接口迁移、地址和路由配置、DNS 文件、NAT 规则及服务启动命令。线路和服务增加后，这些脚本容易重复，配置也容易遗漏。

`netns-r` 将这些步骤集中到配置文件和统一的启动流程中：

| 场景 | 程序提供的能力 |
| --- | --- |
| 同一台主机接入多个 ISP，需要分别运行探测或业务服务 | 为各组服务创建网络命名空间，分别配置接口、地址、默认路由和 DNS |
| 同一套服务要在多个网络环境中运行 | 复用默认服务列表，用 `$(netns)`、`$(service)` 生成命令和日志路径，按需覆盖单个命名空间的服务 |
| 测试需要独立的地址与路由环境 | 创建 veth 设备对；需要外网访问时，可指定主机出口并配置 NAT |
| 服务退出后需要重新启动 | 提供前台监控模式，检查子进程退出并尝试重启；也支持只启动一次 |

例如，一台主机连接三条运营商线路，可以为每条线路分配专用接口和命名空间，再在三个命名空间中启动同一套探测服务。服务沿各自的路由访问网络，日志按命名空间区分，便于对比不同线路的表现。

## 主要特点

- **网络与服务一起配置**：从命名空间、接口、地址和路由，到 DNS 和服务启动，使用同一份 YAML 描述。
- **两种接口模式**：raw-dev 将已有接口移入命名空间，适合专用线路；veth 创建主机与命名空间之间的虚拟链路，可结合出口 NAT 使用。
- **支持双栈网络**：配置 IPv4、IPv6 地址及默认路由，并按配置添加对应的 NAT 规则。
- **减少重复配置**：支持默认网关、DNS、出口和服务列表，以及命名空间级覆盖；服务命令与日志路径支持变量替换。
- **轻量进程管理**：内置类似 supervisord 的轻量进程管理功能，启用监控后，受管程序退出时可自动重新启动，无需引入额外的进程管理依赖。

网络命名空间提供网络栈隔离，不提供完整的容器文件系统或权限隔离。多出口效果依赖正确的接口和路由配置，程序不负责自动选路、负载均衡或线路故障切换。


## 快速开始

```sh
cargo build --release --locked
./target/release/netns-r --help
cp conf/config.example.simple.yaml conf/config.yaml
```

`conf/` 提供两份示例：

- `config.example.simple.yaml`：最小示例，创建一个 veth 命名空间并启动 `sleep` 服务，不配置外网 NAT；
- `config.example.full.yaml`：带注释的完整示例，使用 raw-dev 模式、多个命名空间、默认 DNS 和默认服务列表；其中的设备名、地址和服务命令需按实际环境修改。

阅读[使用指南](docs/usage.md)，确认命名空间名和网段没有与现有环境冲突后，在隔离的 Linux 测试机运行：

```sh
sudo ./target/release/netns-r --config conf/config.yaml
```

简单示例只验证命名空间内部的网络配置。实际运行需要 root 和 iproute2；配置 NAT 时还需要 iptables，IPv6 NAT 需要 ip6tables。

当前版本仍是原型：重复启动、失败回滚和 dry-run 存在已知问题。`--dry-run` 目前不保证无副作用；请勿把它用于生产环境的安全预检查。详见[已知问题](docs/known-issues.md)。

## 文档

- [使用指南](docs/usage.md)：配置、CLI、运行和清理；
- [设计说明](docs/design.md)：模块、执行流程和资源生命周期；
- [开发与验证](docs/verification.md)：本机检查和隔离 Linux 验证；
- [已知问题与后续工作](docs/known-issues.md)：发布前需要完成的工作；
- [更新历史](docs/history.md)：行为变化和整理记录。

## 构建与验证

```sh
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo build --release --locked
```

项目使用 `Cargo.lock` 锁定依赖。macOS 上的编译和基础测试不能替代 Linux 网络验证。Docker 构建入口见[开发文档](docs/verification.md)。

## 许可证

`Cargo.toml` 声明 MIT；独立许可证文件及版权声明尚待发布前确认。

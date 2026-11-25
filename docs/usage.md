# 使用指南

## 1. 准备配置

从项目根目录执行：

```sh
cp conf/config.example.simple.yaml conf/config.yaml
cargo build --release --locked
```

`conf/` 提供两份示例：

- `config.example.simple.yaml`：最小 veth 示例，一个命名空间、一个 `sleep` 服务，不配置 NAT，可在隔离环境直接运行；
- `config.example.full.yaml`：带注释的完整示例，raw-dev 模式、三个命名空间、默认网关/DNS/服务列表；设备名、地址和服务命令需按实际环境修改。

`conf/config.yaml` 是本地配置，不纳入 Git。若已有自己的配置，请保留并修改它，不要直接覆盖。程序默认寻找当前工作目录中的 `netns-r.yaml`；本文统一显式传入 `--config conf/config.yaml`。

实际网络操作仅用于 Linux。需要 root、iproute2；使用 NAT 还需要 iptables，IPv6 NAT 还需要 ip6tables。全局日志文件的父目录须预先存在。

## 2. 配置结构

顶层包含 `global`、`default` 和 `netns-list`。布尔字段支持 `true/false` 和字符串 `yes/no`。建议使用标准 YAML 布尔值。

| 字段 | 当前行为 |
| --- | --- |
| `global.supervisord.enable` | `true`：前台监控服务；`false`：启动后主进程退出 |
| `global.supervisord.check-interval` | 服务检查周期，单位秒，必须大于 0 |
| `global.log.level` | `debug`、`info`、`error`；其他值回退到 `info` |
| `global.log.path` | 主日志路径，以追加方式写入 |
| `default.use-raw-dev` | 全局选择 raw-dev 或 veth 模式 |
| `default.outbound-dev` | veth 模式的默认 NAT 出口，可不设置 |
| `default.network` | 默认网关和 DNS；无默认值时使用 `{}` |
| `default.service-list` | 命名空间没有自己的服务列表时使用，可为 `[]` |
| `netns-list[].name` | 命名空间名称，必须自行确保唯一且不含路径字符 |
| `netns-list[].network` | 接口、地址、网关、DNS 和可选出口覆盖值 |
| `netns-list[].service-list` | 整体覆盖默认服务列表，`[]` 表示不启动服务 |

当前解析器接受 `default.log-file` 和命名空间级 `env`，但运行代码没有应用它们。请在每个服务上显式配置 `log-file` 和 `env`。

### veth 模式

示例中的 `veth-demo` 位于命名空间内，自动创建的 `veth-demo-peer` 位于主机侧。主机端根据网关和命名空间地址的前缀配置地址。选择独立且不冲突的网段，网关应在对应子网内。

接口名及自动追加 `-peer` 后的名称都应满足 Linux 接口名称长度限制（最多 15 字节）。

要使用 IPv6，在 `network` 中增加：

```yaml
ipv6-address: fd00:203::2/64
ipv6-gateway: fd00:203::1
```

要配置外网 NAT，在 `default` 或当前命名空间的 `network` 中设置 `outbound-dev`。程序将添加 MASQUERADE 规则并开启主机转发；不会自动解决主机防火墙的 FORWARD 策略。当前 NAT 路径要求配置 IPv4 地址，不支持纯 IPv6 NAT 配置。

### raw-dev 模式

把 `default.use-raw-dev` 设为 `true`，并为各命名空间设置 `network.raw-dev`。程序把已有接口移入命名空间，不创建 veth/NAT。仅使用专门用于隔离的接口，避免移动正在承载管理连接的网卡。

清理时程序尝试将接口移回 PID 1 的网络命名空间。在容器中，这不一定是物理宿主机网络空间。

### 网关与 DNS

命名空间级的网关、出口优先于 `default`。DNS 未配置时继承 `default.network.dns-servers`；显式 `[]` 禁用该命名空间的专用 DNS 配置。

有 DNS 配置时写入 `/etc/netns/<name>/resolv.conf`，并附加 `timeout:1 attempts:1 rotate single-request-reopen`。无 DNS 时会移除已有的专用 resolv.conf；不会关闭网络本身的 DNS 能力。

## 3. 服务配置

```yaml
service-list:
  - name: idle
    command: "sleep 86400"
    log-file:
      path: /tmp/netns-$(netns)-$(service).log
    env:
      - name: EXAMPLE_VALUE
        value: demo
```

命令通过 `ip netns exec <name> sh -c <command>` 执行，配置文件应视为可执行代码，只加载可信配置。命令和日志路径支持 `$(netns)`、`$(service)` 替换。

服务环境自动加入 `NETNS_NAME`、`SERVICE_NAME`，并覆盖服务中同名的环境变量。环境值本身不做模板替换。

服务的 `log-file: false`、`log-file: no`、`null` 或省略该字段，都会关闭该服务的文件重定向；子进程标准输出和错误默认不会显示在主程序终端。需要记录输出时请显式指定服务日志路径。

## 4. CLI

| 参数 | 默认值 / 含义 |
| --- | --- |
| `-c, --config <PATH>` | `netns-r.yaml` |
| `-l, --log-level <LEVEL>` | 未指定时使用配置的日志级别 |
| `--dry-run` | 输出计划命令；当前仍存在副作用，见已知问题 |
| `--clean` | 按配置清理命名空间、进程、DNS 和相关网络资源 |
| `--pidfile <PATH>` | `/run/netns-r.pid` |
| `--statefile <PATH>` | `/run/netns-r.state` |
| `-h, --help` | 显示帮助 |
| `-V, --version` | 显示版本 |

在隔离 Linux 环境运行：

```sh
sudo ./target/release/netns-r --config conf/config.yaml
```

监控模式下主进程持续运行。SIGINT/SIGTERM 通知监控循环退出，随后执行清理；当前清理直接 SIGKILL 命名空间内进程，不提供服务优雅退出等待期。SIGHUP 只记录“尚未实现重载”，不重新读取配置。

单次模式下主进程启动子进程后退出，命名空间和服务保留；它不等待服务就绪，也不保证服务实际启动成功。需要显式清理：

```sh
sudo ./target/release/netns-r --config conf/config.yaml --clean
```

不要在监控实例仍运行时另起实例或执行清理，也不要将 `--clean` 与 `--dry-run` 组合。`--clean` 可能因命名空间已经不存在而返回错误；目前不保证幂等。

## 5. 运行边界

程序会替换同名命名空间并杀死其中的进程，写入 DNS 和日志文件；NAT 模式修改主机防火墙和转发开关。清理不恢复先前的转发开关，也没有完整保存并恢复原网络配置。

当前版本没有可用于生产环境的无副作用预检查命令。部署前先在隔离 Linux 环境按[验证指南](verification.md)测试，并查看[发布前问题清单](known-issues.md)。

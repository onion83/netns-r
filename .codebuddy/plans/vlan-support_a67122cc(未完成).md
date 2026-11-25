---
name: vlan-support
overview: 为 netns-r 增加 802.1Q VLAN 子接口支持。难度评估：低。第一版仅支持 raw-dev 模式下在主机物理网卡上创建 VLAN 子接口并移入命名空间，预计改动 config.rs（加字段）、netns.rs（新增 configure_vlan_device + 分发逻辑）、env_check.rs（可选检测 8021q 模块），约 100-150 行，无新增依赖。
todos:
  - id: add-vlan-config-fields
    content: 在 config.rs 的 NetnsNetworkConfig 增加 vlan-id/vlan-parent-dev 字段，并在 validate_config 中增加 VLAN 校验逻辑（vid 1-4094、parent-dev 必填、与 use-raw-dev 关系）
    status: pending
  - id: implement-vlan-device
    content: 在 netns.rs 新增 configure_vlan_device 函数，修改 configure_network 分发逻辑，并适配 cleanup_namespaces 清理逻辑
    status: pending
    dependencies:
      - add-vlan-config-fields
  - id: add-vlan-env-check
    content: 在 env_check.rs 增加可选的 8021q 内核模块检测（modprobe -n 8021q）
    status: pending
    dependencies:
      - add-vlan-config-fields
  - id: update-config-examples
    content: 在 example-netns.yaml 和 test.yaml 中新增 VLAN 配置用例
    status: pending
    dependencies:
      - add-vlan-config-fields
  - id: build-and-test-vlan
    content: 构建 arm64 runtime 镜像并在 privileged 容器中测试 VLAN 子接口创建、IP 配置和网络连通性
    status: pending
    dependencies:
      - implement-vlan-device
      - add-vlan-env-check
      - update-config-examples
---

## 用户需求

评估 netns-r 支持 VLAN 的难度，并给出实施方案。

## 产品概述

在现有的 raw-dev（直通模式）和 veth（NAT 模式）两种网络模式基础上，新增 VLAN 子接口模式。该模式允许在主机物理网卡上创建 802.1q VLAN 子接口，将其移入网络命名空间，实现多个命名空间共享同一物理网卡的不同 VLAN，适用于多 ISP 场景下运营商线路通过 VLAN tag 隔离的部署环境。

## 核心功能

- 配置文件支持 `vlan-id`（1-4094）和 `vlan-parent-dev`（父设备名）字段
- 自动在主机端创建 VLAN 子接口（`ip link add link <parent> name <subif> type vlan id <vid>`）
- 启用父设备并将 VLAN 子接口移入命名空间
- 后续 IP 地址/路由/DNS/NAT 配置复用现有逻辑
- 清理时 VLAN 子接口随命名空间删除自动回收

## 难度评估：低

| 维度 | 难度 | 说明 |
| --- | --- | --- |
| 配置层 (config.rs) | 低 | `NetnsNetworkConfig` 增加 2 个 `Option` 字段 + 验证逻辑，向后兼容 |
| 网络层 (netns.rs) | 低-中 | 新增 `configure_vlan_device()` 函数（约 30 行）+ `configure_network()` 分发逻辑 |
| 清理逻辑 (netns.rs) | 极低 | VLAN 子接口移入 ns 后，删除 ns 自动清理；父设备保留主机无需清理 |
| 依赖 (Cargo.toml) | 无 | 不需要新增 crate，iproute2 原生支持 vlan 类型 |
| 环境检查 (env_check.rs) | 低 | 可选：检测内核 8021q 模块是否加载 |
| 测试 | 中 | 容器环境需 `--cap-add NET_ADMIN` 且内核支持 802.1q |


## 技术栈

- 现有技术栈：Rust + serde_yaml + clap + iproute2 命令封装
- 无需新增依赖，VLAN 子接口通过 `ip link add link ... type vlan id ...` 命令实现

## 实现方案

### 关键设计决策

1. **独立函数不复用 `configure_raw_device`**：现有 `configure_raw_device` 会把整个设备 `ip link set dev X netns N` 移入命名空间。VLAN 模式下父设备必须留在主机（多命名空间共享同一物理网卡的不同 VLAN），因此需要独立的 `configure_vlan_device`。
2. **第一版范围限定为 raw-dev 模式 + VLAN 子接口**：veth+VLAN trunk（需 Linux bridge）和 QinQ（802.1ad）暂不支持，避免过度设计。
3. **子接口命名**：使用配置中的 `raw-dev` 字段作为移入命名空间后的设备名（如 `eth0.100`），若未配置则自动生成 `<parent>.<vid>`。

### 命令流程

```
ip link add link <parent> name <subif> type vlan id <vid>   # 主机创建 VLAN 子接口
ip link set <parent> up                                       # 主机启用父设备（若未启用）
ip link set <subif> netns <ns>                                # 移入命名空间
ip netns exec <ns> ip link set <subif> up                     # 命名空间内启用
```

后续 IP 地址/路由/DNS/NAT 配置完全复用现有 `configure_ip_address`、`configure_default_route`、`configure_dns`、`configure_nat` 逻辑。

### 风险与注意点

- **容器测试**：Docker 默认可能不支持 802.1q，需 `--cap-add NET_ADMIN` 且宿主内核加载 8021q 模块
- **父设备重复启用**：多个命名空间共享同一父设备时，父设备只需启用一次，重复 up 应 warn 而非 error
- **MAC 绑定**：ISP 场景可能需独立 MAC（`ip link set address`），作为后续可选功能

## 目录结构

```
src/
├── config.rs       # [MODIFY] NetnsNetworkConfig 增加 vlan-id/vlan-parent-dev 字段 + validate_config 增加 VLAN 校验
├── netns.rs        # [MODIFY] configure_network 增加 VLAN 分发 + 新增 configure_vlan_device() + cleanup 可选清理子接口
├── env_check.rs    # [MODIFY] 可选：增加 8021q 内核模块检测
example-netns.yaml  # [MODIFY] 新增 VLAN 配置用例
test.yaml           # [MODIFY] 新增 VLAN 测试命名空间（可选）
```

## 关键代码结构

`NetnsNetworkConfig` 新增字段：

```rust
#[serde(rename = "vlan-id")]
pub vlan_id: Option<u16>,
#[serde(rename = "vlan-parent-dev")]
pub vlan_parent_dev: Option<String>,
```

`configure_network` 分发逻辑：当 `vlan_id` 存在时走 `configure_vlan_device`，否则保持现有 raw-dev/veth 流程。
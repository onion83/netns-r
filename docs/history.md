# 更新历史

## 未提交：2026-11-26 示例配置拆分

- `conf/config.example.yaml` 重命名为 `conf/config.example.simple.yaml`。
- 新增 `conf/config.example.full.yaml`：带注释的完整示例（raw-dev 模式、三个命名空间、默认网关/DNS/服务列表）。
- 同步更新 README、使用、设计、验证文档，以及 runtime 镜像的示例配置路径和集成测试读取的示例路径。

## 未提交：2026-11-25 仓库整理

- 整理前的项目文件已保存到 `prototype` 分支，快照提交 `3a4d5cc`。
- 新建根目录 README，将文档集中为使用、设计、验证、历史和待办。
- 通用示例移至 `conf/config.example.yaml`；原 `prod.yaml` 留作本地忽略的 `conf/config.yaml`。
- 合并重复的完成报告和测试总结；删除过时的 CodeBuddy 指南及失效 shell 脚本。
- 简化 Makefile，移除本机部署命令，更新 Docker 配置引用。
- 四个 Dockerfile 统一移入 `docker/`，设计文档增加主流程和服务监控 Mermaid 图。
- 保留原型的依赖锁定文件，调整忽略规则与 Docker 构建上下文。
- Rust 运行逻辑不变；已有缺陷明确记录，不将历史测试报告视为本轮验证结果。

## 2026-06-20：信号和网络资源清理

对应提交 `3876c00`：信号处理、进程清理和网络资源配置修复。当前 SIGINT/SIGTERM 设置停止标记；SIGHUP 仅标记并提示尚未实现重载。清理在删除命名空间前终止其中进程，raw-dev 模式尝试移回设备。

## 2026-06-19：配置和服务管理修复

对应提交 `f6ec679`：调整 NAT 源地址、日志路径、环境变量注入、子进程状态检测及 IPv6 NAT 等逻辑；随后 `d508902` 增加原型路线图。

## 2026-06-19：初始原型

对应提交 `7b8acf7`：YAML 配置、raw-dev/veth、服务启动与监控、命名空间网络配置及早期测试与 Docker 构建文件。

更详细的原始报告、计划、配置和旧测试脚本可从 `prototype` 分支读取。历史记录描述当时的改动，不代表当前行为已经完整验证。

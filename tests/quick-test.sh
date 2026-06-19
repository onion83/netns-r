#!/bin/bash
# 快速测试脚本 - 用于快速验证测试环境

set -e

# 颜色定义
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

# 日志函数
log_info() {
    echo -e "${GREEN}[✓]${NC} $1"
}

log_error() {
    echo -e "${RED}[✗]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[!]${NC} $1"
}

echo "=== netns-r 测试环境快速检查 ==="
echo ""

# 1. 检查 Rust 工具链
echo "1. 检查 Rust 工具链..."
if command -v cargo &> /dev/null; then
    log_info "Cargo 已安装: $(cargo --version)"
else
    log_error "Cargo 未安装"
    exit 1
fi

if command -v rustc &> /dev/null; then
    log_info "Rustc 已安装: $(rustc --version)"
else
    log_error "Rustc 未安装"
    exit 1
fi

# 2. 检查系统工具
echo ""
echo "2. 检查系统工具..."

if command -v ip &> /dev/null; then
    log_info "ip 命令可用: $(ip --version 2>&1 | head -1)"
else
    log_warn "ip 命令不可用 (iproute2 未安装)"
fi

if command -v iptables &> /dev/null; then
    log_info "iptables 命令可用"
else
    log_warn "iptables 命令不可用"
fi

# 3. 检查权限
echo ""
echo "3. 检查权限..."
if [[ $EUID -eq 0 ]]; then
    log_info "当前具有 root 权限"
else
    log_warn "当前无 root 权限 (功能测试需要 root)"
fi

# 4. 检查配置文件
echo ""
echo "4. 检查配置文件..."

if [[ -f "test.yaml" ]]; then
    log_info "test.yaml 配置文件存在"
else
    log_error "test.yaml 配置文件不存在"
fi

if [[ -f "example-netns.yaml" ]]; then
    log_info "example-netns.yaml 配置文件存在"
else
    log_warn "example-netns.yaml 配置文件不存在"
fi

# 5. 检查测试脚本
echo ""
echo "5. 检查测试脚本..."

if [[ -f "test.sh" ]]; then
    log_info "test.sh 测试脚本存在"
    if [[ -x "test.sh" ]]; then
        log_info "test.sh 测试脚本有执行权限"
    else
        log_warn "test.sh 测试脚本无执行权限"
    fi
else
    log_error "test.sh 测试脚本不存在"
fi

# 6. 检查项目结构
echo ""
echo "6. 检查项目结构..."

if [[ -f "Cargo.toml" ]]; then
    log_info "Cargo.toml 项目文件存在"
else
    log_error "Cargo.toml 项目文件不存在"
fi

if [[ -d "src" ]]; then
    log_info "src 源代码目录存在"
    file_count=$(find src -name "*.rs" | wc -l)
    log_info "Rust 源文件数量: $file_count"
else
    log_error "src 源代码目录不存在"
fi

if [[ -f "Makefile" ]]; then
    log_info "Makefile 存在"
else
    log_warn "Makefile 不存在"
fi

# 7. 检查编译状态
echo ""
echo "7. 检查编译状态..."

if [[ -d "target" ]]; then
    log_info "target 目录存在"
    
    if [[ -f "target/release/netns-r" ]]; then
        log_info "release 版本已编译"
    else
        log_warn "release 版本未编译"
    fi
    
    if [[ -f "target/debug/netns-r" ]]; then
        log_info "debug 版本已编译"
    else
        log_warn "debug 版本未编译"
    fi
else
    log_warn "target 目录不存在 (项目未编译)"
fi

# 8. 检查测试环境
echo ""
echo "8. 检查测试环境..."

# 检查是否有测试命名空间存在
if ip netns list 2>/dev/null | grep -q "test-ns"; then
    log_warn "检测到测试命名空间存在"
    ip netns list | grep "test-ns"
else
    log_info "无测试命名空间存在"
fi

# 检查日志文件
if ls /tmp/netns-test-*.log 2>/dev/null; then
    log_warn "检测到测试日志文件"
else
    log_info "无测试日志文件"
fi

# 9. 提供下一步建议
echo ""
echo "=== 测试环境检查完成 ==="
echo ""
echo "建议的下一步操作:"
echo ""
echo "1. 编译项目:"
echo "   make build"
echo ""
echo "2. 运行单元测试:"
echo "   make test"
echo ""
echo "3. 运行功能测试（需要 root 权限）:"
echo "   sudo make test-functional"
echo ""
echo "4. 查看测试状态:"
echo "   make test-status"
echo ""
echo "5. 清理测试环境:"
echo "   sudo make test-clean"
echo ""
echo "6. 查看详细测试说明:"
echo "   cat README-TEST.md"
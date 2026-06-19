#!/bin/bash
# netns-r 功能测试脚本
# 使用方法: ./test.sh [test|clean|status]

set -e

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 日志函数
log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

log_debug() {
    echo -e "${BLUE}[DEBUG]${NC} $1"
}

# 检查是否在正确的目录
check_directory() {
    if [[ ! -f "Cargo.toml" ]]; then
        log_error "请在 netns-r 项目根目录运行此脚本"
        exit 1
    fi
}

# 检查是否有 root 权限（创建 netns 需要）
check_privileges() {
    if [[ $EUID -ne 0 ]]; then
        log_warn "创建网络命名空间需要 root 权限"
        log_warn "请使用 sudo 运行此脚本"
        exit 1
    fi
}

# 清理之前的测试环境
cleanup() {
    log_info "清理之前的测试环境..."
    
    # 删除测试命名空间
    for ns in test-ns1 test-ns2 test-ns3; do
        if ip netns list | grep -q "$ns"; then
            log_debug "删除命名空间: $ns"
            ip netns delete "$ns" 2>/dev/null || true
        fi
    done
    
    # 清理日志文件
    log_info "清理测试日志文件..."
    rm -f /tmp/netns-r-test.log
    rm -f /tmp/netns-test-*.log
    
    # 清理 DNS 配置目录
    for ns in test-ns1 test-ns2 test-ns3; do
        if [[ -d "/etc/netns/$ns" ]]; then
            log_debug "清理 DNS 配置: /etc/netns/$ns"
            rm -rf "/etc/netns/$ns"
        fi
    done
    
    log_info "清理完成"
}

# 编译项目
build() {
    log_info "编译 netns-r..."
    cargo build --release
    
    if [[ $? -eq 0 ]]; then
        log_info "编译成功"
    else
        log_error "编译失败"
        exit 1
    fi
}

# 运行功能测试
run_test() {
    log_info "开始功能测试..."
    
    # 首先清理
    cleanup
    
    # 编译
    build
    
    # 运行 dry-run 模式测试
    log_info "测试 1: dry-run 模式..."
    ./target/release/netns-r --config test.yaml --dry-run --log-level debug
    
    if [[ $? -eq 0 ]]; then
        log_info "dry-run 测试通过"
    else
        log_error "dry-run 测试失败"
        exit 1
    fi
    
    # 运行实际测试（需要 root 权限）
    log_info "测试 2: 实际运行（需要 root 权限）..."
    check_privileges
    
    ./target/release/netns-r --config test.yaml --log-level debug
    
    if [[ $? -eq 0 ]]; then
        log_info "实际运行测试完成"
    else
        log_error "实际运行测试失败"
        exit 1
    fi
    
    # 验证结果
    verify_results
    
    log_info "所有功能测试完成！"
}

# 验证测试结果
verify_results() {
    log_info "验证测试结果..."
    
    # 检查命名空间是否创建
    for ns in test-ns1 test-ns2 test-ns3; do
        if ip netns list | grep -q "$ns"; then
            log_info "✓ 命名空间 $ns 创建成功"
        else
            log_error "✗ 命名空间 $ns 创建失败"
            exit 1
        fi
    done
    
    # 检查网络接口
    for ns in test-ns1 test-ns2 test-ns3; do
        if ip netns exec "$ns" ip link show lo | grep -q "state UP"; then
            log_info "✓ 命名空间 $ns 的 loopback 接口已启用"
        else
            log_warn "✗ 命名空间 $ns 的 loopback 接口未启用"
        fi
    done
    
    # 检查日志文件
    if [[ -f "/tmp/netns-r-test.log" ]]; then
        log_info "✓ 主日志文件创建成功"
    else
        log_warn "✗ 主日志文件未创建"
    fi
    
    # 检查服务日志
    for ns in test-ns1 test-ns2 test-ns3; do
        if [[ -f "/tmp/netns-test-${ns}-echo-test.log" ]]; then
            log_info "✓ 命名空间 $ns 的 echo-test 日志创建成功"
            log_debug "日志内容: $(cat /tmp/netns-test-${ns}-echo-test.log)"
        else
            log_warn "✗ 命名空间 $ns 的 echo-test 日志未创建"
        fi
    done
    
    # 测试命令执行
    log_info "测试命名空间内的命令执行..."
    
    for ns in test-ns1 test-ns2 test-ns3; do
        log_debug "在命名空间 $ns 中执行命令..."
        
        # 测试 echo 命令
        result=$(ip netns exec "$ns" echo "test from $ns")
        if [[ "$result" == "test from $ns" ]]; then
            log_info "✓ 命名空间 $ns 命令执行正常"
        else
            log_error "✗ 命名空间 $ns 命令执行失败"
            exit 1
        fi
    done
    
    log_info "所有验证通过！"
}

# 显示状态
show_status() {
    log_info "当前状态:"
    
    # 显示命名空间
    echo "网络命名空间:"
    ip netns list | grep -E "test-ns[123]" || echo "  无测试命名空间"
    
    # 显示日志文件
    echo "日志文件:"
    ls -la /tmp/netns-test-*.log 2>/dev/null || echo "  无日志文件"
    
    # 显示主日志
    if [[ -f "/tmp/netns-r-test.log" ]]; then
        echo "主日志文件:"
        tail -10 /tmp/netns-r-test.log
    fi
}

# 显示帮助
show_help() {
    echo "netns-r 功能测试脚本"
    echo ""
    echo "使用方法:"
    echo "  ./test.sh [命令]"
    echo ""
    echo "命令:"
    echo "  test     运行完整功能测试（默认）"
    echo "  clean    清理测试环境"
    echo "  status   显示当前状态"
    echo "  help     显示此帮助信息"
    echo ""
    echo "示例:"
    echo "  sudo ./test.sh test      # 运行完整测试"
    echo "  sudo ./test.sh clean     # 清理测试环境"
    echo "  ./test.sh status         # 查看状态"
}

# 主函数
main() {
    check_directory
    
    case "${1:-test}" in
        "test")
            run_test
            ;;
        "clean")
            cleanup
            ;;
        "status")
            show_status
            ;;
        "help"|"-h"|"--help")
            show_help
            ;;
        *)
            log_error "未知命令: $1"
            show_help
            exit 1
            ;;
    esac
}

# 运行主函数
main "$@"
#!/bin/bash
# netns-r 容器测试脚本
# 使用方法: ./container-test.sh [build|test|clean]

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

log_step() {
    echo -e "${BLUE}[STEP]${NC} $1"
}

# 检查 Docker 是否可用
check_docker() {
    if ! command -v docker &> /dev/null; then
        log_error "Docker 未安装或不在 PATH 中"
        exit 1
    fi
    
    if ! docker info &> /dev/null; then
        log_error "Docker 服务未运行"
        exit 1
    fi
    
    log_info "Docker 可用"
}

# 构建 Docker 镜像
build_image() {
    log_step "构建 Docker 镜像..."
    docker build -t netns-r-builder .
    log_info "Docker 镜像构建成功"
}

# 运行容器测试
run_test() {
    log_step "运行容器测试..."
    
    docker run --rm --privileged -v "$(pwd):/workspace" netns-r-builder bash -c "
        echo '=== Building netns-r ==='
        cargo build 2>&1 | tail -5
        
        echo '=== Running netns-r to create namespaces ==='
        ./target/debug/netns-r --config test.yaml 2>&1 | grep -E '(INFO|ERROR|WARN)'
        
        echo '=== Verifying namespace creation ==='
        ip netns list
        
        echo '=== Verifying network configuration in test-ns1 ==='
        ip netns exec test-ns1 ip addr show | grep -E 'inet|lo|veth'
        
        echo '=== Verifying DNS configuration ==='
        for ns in test-ns1 test-ns2 test-ns3; do
            echo \"--- \$ns DNS ---\"
            cat /etc/netns/\$ns/resolv.conf 2>/dev/null || echo 'No DNS config'
        done
        
        echo '=== Verifying service logs ==='
        ls -la /tmp/netns-test-*.log 2>/dev/null || echo 'No service logs found'
        
        echo '=== Testing command execution in namespace ==='
        ip netns exec test-ns1 echo 'Hello from test-ns1'
        
        echo '=== Cleaning up ==='
        for ns in test-ns1 test-ns2 test-ns3; do
            ip netns delete \$ns 2>/dev/null || true
        done
        echo 'Cleanup completed'
    "
    
    log_info "容器测试完成"
}

# 运行 dry-run 测试
run_dry_run() {
    log_step "运行 dry-run 测试..."
    
    docker run --rm --privileged -v "$(pwd):/workspace" netns-r-builder bash -c "
        echo '=== Building netns-r ==='
        cargo build 2>&1 | tail -5
        
        echo '=== Running dry-run ==='
        ./target/debug/netns-r --config test.yaml --dry-run 2>&1
    "
    
    log_info "dry-run 测试完成"
}

# 清理资源
clean() {
    log_step "清理 Docker 资源..."
    
    # 删除容器
    docker ps -a --filter ancestor=netns-r-builder -q | xargs -r docker rm -f 2>/dev/null || true
    
    # 删除镜像
    docker rmi netns-r-builder 2>/dev/null || true
    
    log_info "清理完成"
}

# 显示帮助
show_help() {
    echo "netns-r 容器测试脚本"
    echo ""
    echo "使用方法:"
    echo "  ./container-test.sh [命令]"
    echo ""
    echo "命令:"
    echo "  build     构建 Docker 镜像"
    echo "  test      运行完整容器测试"
    echo "  dry-run   运行 dry-run 测试"
    echo "  clean     清理 Docker 资源"
    echo "  help      显示此帮助信息"
    echo ""
    echo "示例:"
    echo "  ./container-test.sh build    # 构建镜像"
    echo "  ./container-test.sh test     # 运行测试"
    echo "  ./container-test.sh clean    # 清理资源"
}

# 主函数
main() {
    check_docker
    
    case "${1:-test}" in
        "build")
            build_image
            ;;
        "test")
            build_image
            run_test
            ;;
        "dry-run")
            build_image
            run_dry_run
            ;;
        "clean")
            clean
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
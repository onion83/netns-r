#!/bin/bash
# 验证功能测试配置脚本

set -e

# 颜色定义
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
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

log_debug() {
    echo -e "${BLUE}[DEBUG]${NC} $1"
}

echo "=== 验证功能测试配置 ==="
echo ""

# 1. 检查测试配置文件
echo "1. 检查测试配置文件..."
if [[ -f "test.yaml" ]]; then
    log_info "test.yaml 配置文件存在"
    
    # 检查配置文件内容
    if grep -q "test-ns1" test.yaml; then
        log_info "包含 test-ns1 命名空间配置"
    else
        log_error "缺少 test-ns1 命名空间配置"
    fi
    
    if grep -q "test-ns2" test.yaml; then
        log_info "包含 test-ns2 命名空间配置"
    else
        log_error "缺少 test-ns2 命名空间配置"
    fi
    
    if grep -q "test-ns3" test.yaml; then
        log_info "包含 test-ns3 命名空间配置"
    else
        log_error "缺少 test-ns3 命名空间配置"
    fi
else
    log_error "test.yaml 配置文件不存在"
fi

# 2. 检查测试脚本
echo ""
echo "2. 检查测试脚本..."

if [[ -f "test.sh" ]]; then
    log_info "test.sh 测试脚本存在"
    
    if [[ -x "test.sh" ]]; then
        log_info "test.sh 有执行权限"
    else
        log_warn "test.sh 无执行权限"
    fi
else
    log_error "test.sh 测试脚本不存在"
fi

if [[ -f "quick-test.sh" ]]; then
    log_info "quick-test.sh 环境检查脚本存在"
    
    if [[ -x "quick-test.sh" ]]; then
        log_info "quick-test.sh 有执行权限"
    else
        log_warn "quick-test.sh 无执行权限"
    fi
else
    log_error "quick-test.sh 环境检查脚本不存在"
fi

# 3. 检查 Makefile 目标
echo ""
echo "3. 检查 Makefile 目标..."

if [[ -f "Makefile" ]]; then
    log_info "Makefile 存在"
    
    if grep -q "test-functional" Makefile; then
        log_info "包含 test-functional 目标"
    else
        log_error "缺少 test-functional 目标"
    fi
    
    if grep -q "test-clean" Makefile; then
        log_info "包含 test-clean 目标"
    else
        log_error "缺少 test-clean 目标"
    fi
    
    if grep -q "test-status" Makefile; then
        log_info "包含 test-status 目标"
    else
        log_error "缺少 test-status 目标"
    fi
    
    if grep -q "test-check" Makefile; then
        log_info "包含 test-check 目标"
    else
        log_error "缺少 test-check 目标"
    fi
else
    log_error "Makefile 不存在"
fi

# 4. 检查文档
echo ""
echo "4. 检查文档..."

if [[ -f "README-TEST.md" ]]; then
    log_info "README-TEST.md 测试文档存在"
else
    log_error "README-TEST.md 测试文档不存在"
fi

if [[ -f "SUMMARY.md" ]]; then
    log_info "SUMMARY.md 项目总结存在"
else
    log_error "SUMMARY.md 项目总结不存在"
fi

# 5. 验证配置文件语法
echo ""
echo "5. 验证配置文件语法..."

if command -v python3 &> /dev/null; then
    if python3 -c "import yaml" 2>/dev/null; then
        log_debug "使用 Python 验证 YAML 语法..."
        if python3 -c "
import yaml
import sys
try:
    with open('test.yaml', 'r') as f:
        yaml.safe_load(f)
    print('YAML 语法验证通过')
except yaml.YAMLError as e:
    print(f'YAML 语法错误: {e}')
    sys.exit(1)
"; then
            log_info "YAML 语法验证通过"
        else
            log_error "YAML 语法验证失败"
        fi
    else
        log_warn "Python yaml 模块不可用，跳过 YAML 验证"
    fi
else
    log_warn "Python3 不可用，跳过 YAML 验证"
fi

# 6. 检查项目编译状态
echo ""
echo "6. 检查项目编译状态..."

if [[ -d "target" ]]; then
    log_info "target 目录存在"
    
    if [[ -f "target/debug/netns-r" ]]; then
        log_info "debug 版本已编译"
        
        # 测试配置文件解析
        echo ""
        echo "7. 测试配置文件解析..."
        if ./target/debug/netns-r --config test.yaml --dry-run 2>/dev/null | grep -q "test-ns1"; then
            log_info "配置文件解析成功，包含 test-ns1"
        else
            log_error "配置文件解析失败"
        fi
    else
        log_warn "debug 版本未编译"
    fi
else
    log_warn "target 目录不存在"
fi

echo ""
echo "=== 验证完成 ==="
echo ""
echo "如果所有检查都通过，可以运行功能测试："
echo "  sudo make test-functional"
echo ""
echo "如果需要修复问题，请参考 README-TEST.md 文档"
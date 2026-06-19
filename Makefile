# netns-r Makefile

# 项目名称
PROJECT_NAME := netns-r

# Rust 编译器
CARGO := cargo

# 安装目录
INSTALL_DIR := /usr/local/bin

# 配置文件目录
CONFIG_DIR := /etc/netns-r

# 默认平台 (arm64 for Apple Silicon development)
PLATFORM := arm64

# 默认目标
.PHONY: all
all: build

# 构建项目
.PHONY: build
build:
	$(CARGO) build

# 构建发布版本
.PHONY: release
release:
	$(CARGO) build --release

# 运行程序
.PHONY: run
run:
	$(CARGO) run -- --config netns.yaml

# 运行 dry-run 模式
.PHONY: dry-run
dry-run:
	$(CARGO) run -- --config netns.yaml --dry-run

# 运行调试模式
.PHONY: debug
debug:
	$(CARGO) run -- --config netns.yaml --log-level debug

# 代码检查
.PHONY: check
check:
	$(CARGO) check

# Clippy 代码检查
.PHONY: clippy
clippy:
	$(CARGO) clippy

# 格式化代码
.PHONY: fmt
fmt:
	$(CARGO) fmt

# 运行测试
.PHONY: test
test:
	$(CARGO) test

# 运行功能测试（需要 root 权限）
.PHONY: test-functional
test-functional:
	@echo "运行功能测试（需要 root 权限）..."
	@chmod +x test.sh
	@sudo ./test.sh test

# 清理测试环境
.PHONY: test-clean
test-clean:
	@echo "清理测试环境..."
	@chmod +x test.sh
	@sudo ./test.sh clean

# 查看测试状态
.PHONY: test-status
test-status:
	@echo "查看测试状态..."
	@chmod +x test.sh
	@./test.sh status

# 快速检查测试环境
.PHONY: test-check
test-check:
	@echo "快速检查测试环境..."
	@chmod +x quick-test.sh
	@./quick-test.sh

# 验证功能测试配置
.PHONY: test-verify
test-verify:
	@echo "验证功能测试配置..."
	@chmod +x verify-test.sh
	@./verify-test.sh

# 在容器内构建项目（使用默认平台 builder 镜像）
.PHONY: build-in-container
build-in-container:
	@echo "在容器内构建项目 (平台: $(PLATFORM))..."
	@docker run --rm -v "$(PWD):/workspace" -w /workspace netns-r-builder.$(PLATFORM) cargo build

# 构建默认平台 builder 镜像
.PHONY: build-builder
build-builder:
	@echo "构建 $(PLATFORM) builder 镜像..."
	@docker build -f Dockerfile.builder.$(PLATFORM) -t netns-r-builder.$(PLATFORM) .

# 构建 x86_64 builder 镜像
.PHONY: build-builder-x86_64
build-builder-x86_64:
	@echo "构建 x86_64 builder 镜像..."
	@docker build -f Dockerfile.builder.x86_64 -t netns-r-builder.x86_64 .

# 构建 arm64 builder 镜像
.PHONY: build-builder-arm64
build-builder-arm64:
	@echo "构建 arm64 builder 镜像..."
	@docker build -f Dockerfile.builder.arm64 -t netns-r-builder.arm64 .

# 构建默认平台 runtime 镜像
.PHONY: build-runtime
build-runtime:
	@echo "构建 $(PLATFORM) runtime 镜像..."
	@docker build -f Dockerfile.builder.$(PLATFORM) -t netns-r-builder.$(PLATFORM) .
	@docker build -f Dockerfile.runtime.$(PLATFORM) -t netns-r-runtime.$(PLATFORM) .

# 构建 x86_64 runtime 镜像
.PHONY: build-runtime-x86_64
build-runtime-x86_64:
	@echo "构建 x86_64 runtime 镜像..."
	@docker build -f Dockerfile.builder.x86_64 -t netns-r-builder.x86_64 .
	@docker build -f Dockerfile.runtime.x86_64 -t netns-r-runtime.x86_64 .
	scp '/Users/onion/OrbStack/docker/images/netns-r-runtime.x86_64:latest/netns-r/netns-r' root@192.168.11.40:/data/apps/netns-r/netns-r
netns-r

# 构建 arm64 runtime 镜像
.PHONY: build-runtime-arm64
build-runtime-arm64:
	@echo "构建 arm64 runtime 镜像..."
	@docker build -f Dockerfile.builder.arm64 -t netns-r-builder.arm64 .
	@docker build -f Dockerfile.runtime.arm64 -t netns-r-runtime.arm64 .

# 完整构建流程：编译、测试、构建 runtime 镜像（默认平台）
.PHONY: test-and-build
test-and-build:
	@echo "开始完整构建流程 (平台: $(PLATFORM))..."
	@echo "步骤 1: 在容器内编译..."
	@make build-in-container
	@echo "步骤 2: 运行测试..."
	@make test-container
	@echo "步骤 3: 构建 runtime 镜像..."
	@make build-runtime
	@echo "构建完成！"

# 运行容器测试
.PHONY: test-container
test-container:
	@echo "运行容器测试 (平台: $(PLATFORM))..."
	@chmod +x container-test.sh
	@./container-test.sh test

# 运行容器 dry-run 测试
.PHONY: test-container-dry-run
test-container-dry-run:
	@echo "运行容器 dry-run 测试 (平台: $(PLATFORM))..."
	@chmod +x container-test.sh
	@./container-test.sh dry-run

# 清理容器资源
.PHONY: test-container-clean
test-container-clean:
	@echo "清理容器资源..."
	@chmod +x container-test.sh
	@./container-test.sh clean

# 清理构建文件
.PHONY: clean
clean:
	$(CARGO) clean

# 安装程序
.PHONY: install
install: release
	@echo "Installing $(PROJECT_NAME) to $(INSTALL_DIR)..."
	@mkdir -p $(INSTALL_DIR)
	@cp target/release/$(PROJECT_NAME) $(INSTALL_DIR)/
	@chmod +x $(INSTALL_DIR)/$(PROJECT_NAME)
	@echo "Installation complete."

# 卸载程序
.PHONY: uninstall
uninstall:
	@echo "Uninstalling $(PROJECT_NAME)..."
	@rm -f $(INSTALL_DIR)/$(PROJECT_NAME)
	@echo "Uninstallation complete."

# 安装配置文件
.PHONY: install-config
install-config:
	@echo "Installing example configuration..."
	@mkdir -p $(CONFIG_DIR)
	@cp example-netns.yaml $(CONFIG_DIR)/netns.yaml.example
	@echo "Example configuration installed to $(CONFIG_DIR)/netns.yaml.example"

# 显示帮助
.PHONY: help
help:
	@echo "netns-r - Network namespace management tool"
	@echo ""
	@echo "Usage:"
	@echo "  make [target]"
	@echo ""
	@echo "Variables:"
	@echo "  PLATFORM       Target platform: arm64 (default) or x86_64"
	@echo ""
	@echo "Targets:"
	@echo "  all              Build the project (default)"
	@echo "  build            Build the project"
	@echo "  release          Build release version"
	@echo "  run              Run the program"
	@echo "  dry-run          Run in dry-run mode"
	@echo "  debug            Run in debug mode"
	@echo "  check            Check the code"
	@echo "  clippy           Run Clippy linter"
	@echo "  fmt              Format the code"
	@echo "  test             Run unit tests"
	@echo "  test-functional  Run functional tests (requires root)"
	@echo "  test-clean       Clean test environment"
	@echo "  test-status      Show test status"
	@echo "  test-check       Quick check test environment"
	@echo "  test-verify      Verify functional test configuration"
	@echo "  build-builder    Build default platform builder image"
	@echo "  build-builder-x86_64  Build x86_64 builder image"
	@echo "  build-builder-arm64   Build arm64 builder image"
	@echo "  build-runtime    Build default platform runtime image"
	@echo "  build-runtime-x86_64  Build x86_64 runtime image"
	@echo "  build-runtime-arm64   Build arm64 runtime image"
	@echo "  test-container   Run container tests"
	@echo "  test-container-dry-run Run container dry-run tests"
	@echo "  test-container-clean Clean container resources"
	@echo "  test-and-build   Complete build: compile, test, build runtime"
	@echo "  clean            Clean build files"
	@echo "  install          Install the program"
	@echo "  uninstall        Uninstall the program"
	@echo "  install-config   Install example configuration"
	@echo "  help             Show this help"
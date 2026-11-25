CARGO ?= cargo
PLATFORM ?= arm64

.PHONY: all build release check test clippy fmt fmt-check clean help build-builder build-runtime
all: build

build:
	$(CARGO) build --locked

release:
	$(CARGO) build --release --locked

check:
	$(CARGO) check --locked

test:
	$(CARGO) test --locked

clippy:
	$(CARGO) clippy --all-targets --locked -- -D warnings

fmt:
	$(CARGO) fmt --all

fmt-check:
	$(CARGO) fmt --all -- --check

clean:
	$(CARGO) clean

build-builder:
	docker build -f docker/Dockerfile.builder.$(PLATFORM) -t netns-r-builder.$(PLATFORM) .

build-runtime: build-builder
	docker build -f docker/Dockerfile.runtime.$(PLATFORM) -t netns-r-runtime.$(PLATFORM) .

help:
	@echo "netns-r: build release check test clippy fmt fmt-check clean"
	@echo "Docker: make build-builder|build-runtime PLATFORM=arm64|x86_64"
	@echo "运行和清理方法见 docs/usage.md；验证边界见 docs/verification.md。"

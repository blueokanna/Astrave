ROOT := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
export CARGO_TARGET_DIR := $(ROOT)/target

.PHONY: web docs build release astrave-server astrave package test fmt clean
web: astrave-server

docs:
	cargo run --locked -p astrave-docs

build:
	cargo build -p astrave-server -p astrave-client

release:
	cargo build --release -p astrave-server -p astrave-client
	@echo ""
	@echo "artifacts:"
	@ls -lh target/release/astrave-server target/release/astrave

astrave-server:
	cargo build --release -p astrave-server
	@echo ""
	@echo "artifact:"
	@ls -lh target/release/astrave-server

astrave:
	cargo build --release -p astrave-client

FLUTTER_PLATFORM ?= $(if $(filter Windows_NT,$(OS)),windows,$(if $(filter Darwin,$(shell uname -s)),macos,linux))

desktop-dev:
	cd app && flutter run -d $(FLUTTER_PLATFORM)

desktop-build:
	cd app && flutter build $(FLUTTER_PLATFORM) --release

desktop-app desktop-dmg: desktop-build
	chmod +x scripts/pack-desktop-macos.sh
	./scripts/pack-desktop-macos.sh
	@echo "app -> dist/Astrave Desktop.app"
	@echo "dmg -> dist/astrave-desktop_*_darwin_*.dmg"

desktop-windows: desktop-build
	chmod +x scripts/pack-desktop-windows.sh
	./scripts/pack-desktop-windows.sh
	@echo "exe/zip -> dist/astrave-desktop_*_windows_*"

desktop-deb: desktop-build
	chmod +x scripts/pack-desktop-linux-deb.sh
	./scripts/pack-desktop-linux-deb.sh
	@echo "deb -> dist/astrave-desktop_*_linux_*.deb"

test:
	cargo test --workspace

docker-context:
	chmod +x scripts/prepare-docker-context.sh scripts/stage-docker-slice.sh
	./scripts/prepare-docker-context.sh --upx

docker-client: docker-context
	docker build --pull=false -f docker/Dockerfile.client -t astrave:local .

docker-server: docker-context
	docker build --pull=false -f docker/Dockerfile.server -t astrave-server:local .

fmt:
	cargo fmt --all

clean:
	cargo clean
	rm -rf docs/build

package: release
	mkdir -p dist
	cp target/release/astrave-server target/release/astrave dist/
	cp -R conf dist/
	@echo "packaged -> dist/"
	@ls -lh dist/

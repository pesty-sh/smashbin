# Build for this Mac into target/. Do not copy the binary onto PATH.
# Intel Macs from 2014 can run macOS 10.12. rustup's standard library is
# stamped 10.12. The cargo first on PATH is Homebrew's rustc, and that
# standard library is stamped 13.0, so this file calls rustup by path.
# Do not set Cargo's default target. This Mac stays the host build.
# The published Mac install is Homebrew: brew install pesty-sh/tap/smashbin.
#
# This is the same shape as qrbin's Makefile. The names are smashbin.

RUSTUP := $(HOME)/.cargo/bin/rustup
# The proxy at ~/.cargo/bin/cargo loads rustup's libLLVM so release
# stripping works. RUSTC is pinned to the same toolchain so a cross
# build cannot pick up Homebrew's rustc from PATH.
RUSTC := $(shell $(RUSTUP) which rustc)
RUSTUP_CARGO := $(HOME)/.cargo/bin/cargo
export RUSTC
export MACOSX_DEPLOYMENT_TARGET := 10.12

.PHONY: build doctor build-silicon build-windows deb linux

.DEFAULT_GOAL := build

build:
	@test -x "$(RUSTC)" || { \
		echo "rustup rustc is missing. Homebrew's rustc stamps macOS 13.0."; \
		exit 1; \
	}
	@test -x "$(RUSTUP_CARGO)" || { \
		echo "rustup cargo is missing."; \
		exit 1; \
	}
	env -u CARGO_TARGET_DIR "$(RUSTUP_CARGO)" build --release
	./packaging/check-macos-minos.sh target/release/smashbin 10.12
	@echo "target/release/smashbin"

# Apple silicon. Output stays in target/. This Intel Mac cannot run it.
# The standard library for this target is stamped macOS 11.0.
build-silicon:
	@test -x "$(RUSTC)" || { echo "rustup rustc is missing."; exit 1; }
	env -u CARGO_TARGET_DIR MACOSX_DEPLOYMENT_TARGET=11.0 "$(RUSTUP_CARGO)" build --release --target aarch64-apple-darwin
	./packaging/check-macos-minos.sh target/aarch64-apple-darwin/release/smashbin 11.0

# Windows 10 and 11, 64-bit Intel and AMD. Does not install.
# rustup target: x86_64-pc-windows-gnu. Linker: mingw-w64.
WINDOWS_TARGET := x86_64-pc-windows-gnu
WINDOWS_LINKER := x86_64-w64-mingw32-gcc

build-windows:
	@test -x "$(RUSTC)" || { echo "rustup rustc is missing."; exit 1; }
	@command -v "$(WINDOWS_LINKER)" >/dev/null 2>&1 || { \
		echo "Windows linker $(WINDOWS_LINKER) is missing."; \
		echo "Install it with: brew install mingw-w64"; \
		exit 1; \
	}
	env -u MACOSX_DEPLOYMENT_TARGET -u CARGO_TARGET_DIR \
		CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER="$(WINDOWS_LINKER)" \
		"$(RUSTUP_CARGO)" build --release --target "$(WINDOWS_TARGET)"
	@file "target/$(WINDOWS_TARGET)/release/smashbin.exe" | grep -q "PE32+" || { \
		echo "target/$(WINDOWS_TARGET)/release/smashbin.exe is not a Windows exe."; \
		exit 1; \
	}
	@file "target/$(WINDOWS_TARGET)/release/smashbin.exe"

doctor:
	@echo "make uses cargo: $(RUSTUP_CARGO)"
	@echo "make uses rustc: $(RUSTC)"
	@"$(RUSTC)" --version
	@echo "Homebrew cargo, not used: /usr/local/bin/cargo"
	@/usr/local/bin/cargo --version
	@echo "Intel deployment target: $(MACOSX_DEPLOYMENT_TARGET)"
	@if command -v "$(WINDOWS_LINKER)" >/dev/null 2>&1; then \
		echo "Windows linker: $$(command -v "$(WINDOWS_LINKER)")"; \
	else \
		echo "Windows linker: missing. brew install mingw-w64"; \
	fi
	@zig="$(HOME)/.cache/smashbin/zig-x86_64-macos-0.16.0/zig"; \
	if [ -x "$$zig" ]; then echo "Linux zig: $$zig"; else echo "Linux zig: downloaded by make linux"; fi
	@if command -v cargo-zigbuild >/dev/null 2>&1; then \
		echo "cargo-zigbuild: $$(command -v cargo-zigbuild)"; \
	else \
		echo "cargo-zigbuild: installed by make linux"; \
	fi
	@echo "Linux glibc floor: $(LINUX_GLIBC) (Ubuntu 18.04). Intel/AMD and ARM."

# Ubuntu 18.04 ships glibc 2.27. A binary linked to that version also runs
# where glibc is newer:
#   Ubuntu 20.04 is 2.31, 22.04 is 2.35, 24.04 is 2.39.
#   Debian 10 is 2.28, 11 is 2.31, 12 is 2.36, 13 is 2.41.
#   Arch Linux is rolling and newer than 2.27.
# Intel and AMD are one x86_64 build. 64-bit ARM is aarch64.
# Does not install, and does not use Docker.
LINUX_GLIBC := 2.27

deb linux:
	./packaging/build-linux-host.sh "$(LINUX_GLIBC)"

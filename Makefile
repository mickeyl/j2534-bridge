SHELL := /bin/sh

CARGO ?= cargo
BUILD_TARGET :=
WIN32_TARGET := i686-pc-windows-msvc
WIN64_TARGET := x86_64-pc-windows-msvc
DEV_BIN_DIR := ../CANcorder/src-tauri/bin
DEVICE ?=
DLL ?=
BITNESS ?=
BAUD ?= 500000
DURATION ?= 10
TIMEOUT_MS ?= 25
BATCH_SIZE ?= 256
MAX_DRAIN_READS ?= 64
INTERFACE ?= j2534
EXTRA ?=

ifeq ($(strip $(DLL)),)
ifneq ($(strip $(DEVICE)),)
SELECTOR := --device-name "$(DEVICE)"
endif
else
SELECTOR := --dll-path "$(DLL)"
endif

ifneq ($(strip $(BITNESS)),)
BITNESS_ARG := --bitness $(BITNESS)
endif

ifeq ($(strip $(BITNESS)),32)
BUILD_TARGET := --target $(WIN32_TARGET)
endif

ifeq ($(strip $(BITNESS)),64)
BUILD_TARGET := --target $(WIN64_TARGET)
endif

DUMP = $(CARGO) run --bin j2534-dump -- $(SELECTOR) $(BITNESS_ARG) --baud-rate $(BAUD) --timeout-ms $(TIMEOUT_MS) --batch-size $(BATCH_SIZE) --max-drain-reads $(MAX_DRAIN_READS) --interface $(INTERFACE)

.PHONY: help test build ensure-bridge ensure-bridge-dev publish-dev list dump dump-std dump-ext dump-both dump-loopback dump-stress-loopback dump-raw dump-isotp native-build native-list native-rx native-rx-hybrid native-rx-v5-6param native-rx-nofilter

help:
	@echo "j2534-bridge test presets"
	@echo ""
	@echo "Variables:"
	@echo "  DEVICE=<registry name>   Select adapter by J2534 registry name"
	@echo "  DLL=<path>               Select adapter by FunctionLibrary path"
	@echo "  BITNESS=32|64            Override bridge bitness"
	@echo "  BAUD=500000              Bus speed"
	@echo "  DURATION=10              Capture length in seconds"
	@echo "  EXTRA='...'              Extra args passed to j2534-dump"
	@echo ""
	@echo "Targets:"
	@echo "  make test                  Run unit tests (protocol + worker)"
	@echo "  make list"
	@echo "  make ensure-bridge-dev     Build and publish fresh dev bridge binaries for CANcorder"
	@echo "  make native-build          Build standalone C J2534 v5.0 test harness"
	@echo "  make native-list           Enumerate devices through the native C harness"
	@echo "  make native-rx             Native C CAN RX test (v5 connect + v5 filter)"
	@echo "  make native-rx-hybrid      v5 connect + v4.04 filter/read (Rust bridge path)"
	@echo "  make native-rx-v5-6param   v5 connect + 6-param filter w/ v5 struct"
	@echo "  make native-rx-nofilter    v5 connect + skip filter (test default pass-through)"
	@echo "  make dump DEVICE='My Adapter'"
	@echo "  make dump-std DEVICE='My Adapter'"
	@echo "  make dump-ext DEVICE='My Adapter'"
	@echo "  make dump-both DEVICE='My Adapter'"
	@echo "  make dump-loopback DEVICE='My Adapter'"
	@echo "  make dump-raw DEVICE='My Adapter'"
	@echo "  make dump-isotp DEVICE='My Adapter'"

test:
	$(CARGO) test

ensure-bridge:
	$(CARGO) build $(BUILD_TARGET) --bin j2534-bridge

ensure-bridge-dev:
	$(CARGO) build --target $(WIN32_TARGET) --bin j2534-bridge
	$(CARGO) build --target $(WIN64_TARGET) --bin j2534-bridge
	@mkdir -p "$(DEV_BIN_DIR)"
	cp "target/$(WIN32_TARGET)/debug/j2534-bridge.exe" "$(DEV_BIN_DIR)/j2534-bridge-32.exe"
	cp "target/$(WIN64_TARGET)/debug/j2534-bridge.exe" "$(DEV_BIN_DIR)/j2534-bridge-64.exe"

publish-dev: ensure-bridge-dev

list: ensure-bridge
	$(CARGO) run --bin j2534-dump -- $(BITNESS_ARG) --list

build:
	$(CARGO) build $(BUILD_TARGET) --bin j2534-bridge --bin j2534-dump

native-build:
	powershell -ExecutionPolicy Bypass -File .\scripts\build-native-j2534-test.ps1 -Arch x64

native-list: native-build
	.\target\native\x64\j2534_v5_can_test.exe list

native-rx: native-build
	.\target\native\x64\j2534_v5_can_test.exe can-rx --open-name "J2534-2:PEAK 0x51" --baud $(BAUD) --duration $(DURATION) --timeout-ms $(TIMEOUT_MS)

native-rx-hybrid: native-build
	.\target\native\x64\j2534_v5_can_test.exe can-rx-hybrid --open-name "J2534-2:PEAK 0x51" --baud $(BAUD) --duration $(DURATION) --timeout-ms $(TIMEOUT_MS)

native-rx-v5-6param: native-build
	.\target\native\x64\j2534_v5_can_test.exe can-rx-v5-6param --open-name "J2534-2:PEAK 0x51" --baud $(BAUD) --duration $(DURATION) --timeout-ms $(TIMEOUT_MS)

native-rx-nofilter: native-build
	.\target\native\x64\j2534_v5_can_test.exe can-rx --open-name "J2534-2:PEAK 0x51" --baud $(BAUD) --duration $(DURATION) --timeout-ms $(TIMEOUT_MS) --skip-filter

dump: ensure-bridge
	$(DUMP) --connect-mode both --clear-buffers --show-version --show-state --ascii --duration-secs $(DURATION) $(EXTRA)

dump-std: ensure-bridge
	$(DUMP) --connect-mode standard --filter "pass:00000000:00000000:false:false" --clear-buffers --show-state --ascii --duration-secs $(DURATION) $(EXTRA)

dump-ext: ensure-bridge
	$(DUMP) --connect-mode extended --filter "pass:00000000:00000000:true:false" --clear-buffers --show-state --ascii --duration-secs $(DURATION) $(EXTRA)

dump-both: ensure-bridge
	$(DUMP) --connect-mode both --filter "pass:00000000:00000000:false:false" --filter "pass:00000000:00000000:true:false" --clear-buffers --show-version --show-state --ascii --duration-secs $(DURATION) $(EXTRA)

dump-loopback: ensure-bridge
	$(DUMP) --connect-mode both --read-mode loopback --set-loopback true --clear-buffers --show-state --ascii --duration-secs $(DURATION) $(EXTRA)

dump-raw: ensure-bridge
	$(DUMP) --connect-mode both --read-mode raw --clear-buffers --duration-secs $(DURATION) $(EXTRA)

dump-stress-loopback: ensure-bridge
	$(DUMP) --connect-mode both --read-mode stress-loopback --clear-buffers --show-state $(EXTRA)

dump-isotp: ensure-bridge
	$(DUMP) --protocol-id 6 --connect-mode both --filter "pass:00000000:00000000:false:false" --filter "pass:00000000:00000000:true:false" --clear-buffers --show-version --show-state --ascii --duration-secs $(DURATION) $(EXTRA)

SHELL := /bin/sh

CARGO ?= cargo
BUILD_TARGET :=
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
BUILD_TARGET := --target i686-pc-windows-msvc
endif

ifeq ($(strip $(BITNESS)),64)
BUILD_TARGET := --target x86_64-pc-windows-msvc
endif

DUMP = $(CARGO) run --bin j2534-dump -- $(SELECTOR) $(BITNESS_ARG) --baud-rate $(BAUD) --timeout-ms $(TIMEOUT_MS) --batch-size $(BATCH_SIZE) --max-drain-reads $(MAX_DRAIN_READS) --interface $(INTERFACE)

.PHONY: help build ensure-bridge list dump dump-std dump-ext dump-both dump-loopback dump-stress-loopback dump-raw dump-isotp

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
	@echo "  make list"
	@echo "  make dump DEVICE='My Adapter'"
	@echo "  make dump-std DEVICE='My Adapter'"
	@echo "  make dump-ext DEVICE='My Adapter'"
	@echo "  make dump-both DEVICE='My Adapter'"
	@echo "  make dump-loopback DEVICE='My Adapter'"
	@echo "  make dump-raw DEVICE='My Adapter'"
	@echo "  make dump-isotp DEVICE='My Adapter'"

ensure-bridge:
	$(CARGO) build $(BUILD_TARGET) --bin j2534-bridge

list: ensure-bridge
	$(CARGO) run --bin j2534-dump -- $(BITNESS_ARG) --list

build:
	$(CARGO) build $(BUILD_TARGET) --bin j2534-bridge --bin j2534-dump

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

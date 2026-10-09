# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Michael Wulff Nielsen
# Build the uSVC game loader from the upstream source with arm-none-eabi-gcc.
#
#   make -C loader          build build/loader/loader.bin and report its size
#   make -C loader HUB=1    the same with USB hub support, in build/loader-hub/
#   make -C loader clean
#
# The loader must fit in the 16384 bytes between 0x2000 and 0x6000, where
# every game starts. Upstream's own build leaves 32 bytes free. This one
# leaves about 450, by leaving out the C library's start-up files (see
# init.c) and by three size options.
#
# HUB=1 builds from a copy of the upstream source with the patches in hub/
# applied: a gamepad or other full-speed device can then be plugged into a
# USB hub (see hub/usb_hub.c). To make room, that loader has no logo and no
# sound.
#
# Nothing here writes to a console.

LOADER   := $(patsubst %/,%,$(dir $(abspath $(lastword $(MAKEFILE_LIST)))))
ROOT     := $(abspath $(LOADER)/..)
UPSTREAM := $(ROOT)/reference/uSVC/software/uSVC_GameLoader
BUILD    ?= $(ROOT)/build/loader$(if $(HUB),-hub)
SLOT     := 16384
# Where the loader's source is compiled from.
SRC      := $(if $(HUB),$(BUILD)/src,$(UPSTREAM))
PATCHES  := $(sort $(wildcard $(LOADER)/hub/*.patch))

CC      := arm-none-eabi-gcc
OBJCOPY := arm-none-eabi-objcopy
OBJDUMP := arm-none-eabi-objdump
SIZE    := arm-none-eabi-size

# Upstream's Atmel Studio settings (-Os, sections, nano and nosys specs),
# plus SMALLER. Each option in SMALLER was measured; others made no
# difference or made the loader bigger.
ARCH    := -mthumb -mcpu=cortex-m0plus
SMALLER := -fno-move-loop-invariants -ffreestanding -fno-tree-loop-distribute-patterns
INCS    := -I$(SRC) -I$(SRC)/usvc_kernel/include -I$(ROOT)/sdk/include -I$(ROOT)/sdk/cmsis
CFLAGS  := -x c $(ARCH) -D__SAMD21E18A__ -DNDEBUG $(INCS) -Os -ffunction-sections -std=gnu99 $(SMALLER)
LDFLAGS := $(ARCH) -nostartfiles -Wl,--start-group -lm -Wl,--end-group -L$(UPSTREAM)/Device_Startup \
           -Wl,--gc-sections -Tsamd21g18a_flash.ld --specs=nano.specs --specs=nosys.specs \
           -Wl,--no-warn-rwx-segments

# The files the upstream project compiles. Its directory holds others
# (printf.c, rand.c, system_samd21.c) that are not part of the loader.
KERNEL_NAMES := audio audioBootloader bootFont8x8reduced defaultSounds mmc font8x8 pff \
                stepTable system USB_HID_Boot_Keyboard USB_HID_Generic_Gamepad usb_host \
                usb_supported_devices usvcUtils vga
MAIN_NAMES   := main $(if $(HUB),,logoTiles usvcLogoMap)
OBJS := $(patsubst %,$(BUILD)/obj/kernel/%.o,$(KERNEL_NAMES)) \
        $(patsubst %,$(BUILD)/obj/main/%.o,$(MAIN_NAMES)) \
        $(BUILD)/obj/startup/startup_samd21.o $(BUILD)/obj/init.o \
        $(if $(HUB),$(BUILD)/obj/usb_hub.o)

ELF := $(BUILD)/loader.elf
BIN := $(BUILD)/loader.bin
LSS := $(BUILD)/loader.lss

.PHONY: all clean
all: $(BIN) $(LSS)
	@$(SIZE) $(ELF)
	@used=$$(($$(wc -c < $(BIN)))); echo "loader: $$used of $(SLOT) bytes, $$(($(SLOT) - used)) free"

# With HUB=1, a copy of the upstream source with the patches applied. patch
# stops the build if upstream has changed under one of them.
STAMP := $(BUILD)/.source
$(STAMP): $(if $(HUB),$(PATCHES) $(wildcard $(UPSTREAM)/*.[ch] $(UPSTREAM)/usvc_kernel/*.[ch]))
	@mkdir -p $(BUILD)
ifneq ($(HUB),)
	@rsync -a --delete --exclude .vs --exclude Debug --exclude Release $(UPSTREAM)/ $(BUILD)/src/
	@for p in $(PATCHES); do patch -s -p1 -d $(BUILD)/src < $$p || exit 1; done
endif
	@touch $@

# Upstream code is not warning-clean; that is not ours to fix.
$(BUILD)/obj/kernel/%.o: $(UPSTREAM)/usvc_kernel/%.c $(STAMP)
	@mkdir -p $(dir $@)
	$(CC) $(CFLAGS) -w -MMD -MP -c $(SRC)/usvc_kernel/$*.c -o $@

$(BUILD)/obj/main/%.o: $(UPSTREAM)/%.c $(STAMP)
	@mkdir -p $(dir $@)
	$(CC) $(CFLAGS) -w -MMD -MP -c $(SRC)/$*.c -o $@

$(BUILD)/obj/startup/%.o: $(UPSTREAM)/Device_Startup/%.c $(STAMP)
	@mkdir -p $(dir $@)
	$(CC) $(CFLAGS) -w -MMD -MP -c $< -o $@

$(BUILD)/obj/init.o: $(LOADER)/init.c $(STAMP)
	@mkdir -p $(dir $@)
	$(CC) $(CFLAGS) -Wall -MMD -MP -c $< -o $@

$(BUILD)/obj/usb_hub.o: $(LOADER)/hub/usb_hub.c $(STAMP)
	@mkdir -p $(dir $@)
	$(CC) $(CFLAGS) -Wall -MMD -MP -c $< -o $@

# The linker script fails the build if the loader outgrows its slot.
$(ELF): $(OBJS)
	$(CC) -o $@ $(OBJS) $(LDFLAGS) -Wl,-Map=$(BUILD)/loader.map

$(BIN): $(ELF)
	$(OBJCOPY) -O binary $< $@

$(LSS): $(ELF)
	$(OBJDUMP) -h -t -S $< > $@

clean:
	rm -rf $(BUILD)

-include $(OBJS:.o=.d)

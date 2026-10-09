# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Michael Wulff Nielsen
# Build the USB hub probe (see hubtest.c) with arm-none-eabi-gcc.
#
#   make -C loader/hubtest          build build/hubtest/hubtest.bin
#   make -C loader/hubtest clean
#
# The probe is the upstream game loader's kernel with hubtest.c in place of
# the loader's menu, linked at the loader's address, 0x2000. It is a little
# larger than the loader's 16384-byte slot and runs on into the space for
# the game. On a console it replaces the game loader and any installed game,
# so keep a backup of the console's flash.
#
# Nothing here writes to a console.

HUBTEST  := $(patsubst %/,%,$(dir $(abspath $(lastword $(MAKEFILE_LIST)))))
ROOT     := $(abspath $(HUBTEST)/../..)
UPSTREAM := $(ROOT)/reference/uSVC/software/uSVC_GameLoader
BUILD    ?= $(ROOT)/build/hubtest
SRC      := $(BUILD)/src

CC      := arm-none-eabi-gcc
OBJCOPY := arm-none-eabi-objcopy
OBJDUMP := arm-none-eabi-objdump
SIZE    := arm-none-eabi-size

# Same options as the loader build (see ../Makefile).
ARCH    := -mthumb -mcpu=cortex-m0plus
SMALLER := -fno-move-loop-invariants -ffreestanding -fno-tree-loop-distribute-patterns
INCS    := -I$(SRC) -I$(SRC)/usvc_kernel/include -I$(ROOT)/sdk/include -I$(ROOT)/sdk/cmsis
CFLAGS  := -x c $(ARCH) -D__SAMD21E18A__ -DNDEBUG $(INCS) -Os -ffunction-sections -std=gnu99 $(SMALLER)
LDFLAGS := $(ARCH) -nostartfiles -Wl,--start-group -lm -Wl,--end-group -L$(SRC) \
           -Wl,--gc-sections -Tsamd21g18a_flash.ld --specs=nano.specs --specs=nosys.specs \
           -Wl,--no-warn-rwx-segments

# The loader's kernel files, without usb_supported_devices.c: hubtest.c has
# its own list of drivers.
KERNEL_NAMES := audio audioBootloader bootFont8x8reduced defaultSounds mmc font8x8 pff \
                stepTable system USB_HID_Boot_Keyboard USB_HID_Generic_Gamepad usb_host \
                usvcUtils vga
OBJS := $(patsubst %,$(BUILD)/obj/kernel/%.o,$(KERNEL_NAMES)) \
        $(BUILD)/obj/startup_samd21.o $(BUILD)/obj/init.o $(BUILD)/obj/hubtest.o

ELF := $(BUILD)/hubtest.elf
BIN := $(BUILD)/hubtest.bin
LSS := $(BUILD)/hubtest.lss
STAMP := $(BUILD)/.kernel

.PHONY: all clean
all: $(BIN) $(LSS)
	@$(SIZE) $(ELF)
	@echo "hubtest: $$(($$(wc -c < $(BIN)))) bytes at 0x2000"

# The kernel includes "../usvc_config.h", so it is copied into the build
# directory next to a changed copy of the loader's configuration: room for a
# hub with four devices and one more pipe, and no tiles beyond the characters
# (which frees the RAM for them). The linker script gets room for 32768
# bytes in the same way.
$(STAMP): $(wildcard $(UPSTREAM)/usvc_kernel/*.[ch]) $(UPSTREAM)/usvc_config.h \
          $(UPSTREAM)/Device_Startup/samd21g18a_flash.ld
	@mkdir -p $(BUILD)/obj/kernel $(SRC)
	@rsync -a --delete $(UPSTREAM)/usvc_kernel/ $(SRC)/usvc_kernel/
	@LC_ALL=C sed -e 's/#define USB_NUMDEVICES 1/#define USB_NUMDEVICES 5/' \
		-e 's/#define NUMBER_OF_USB_PIPES 3/#define NUMBER_OF_USB_PIPES 4/' \
		-e 's/#define MAX_TILES (210)/#define MAX_TILES (104)/' \
		$(UPSTREAM)/usvc_config.h > $(SRC)/usvc_config.h
	@LC_ALL=C grep -q '#define USB_NUMDEVICES 5' $(SRC)/usvc_config.h
	@LC_ALL=C grep -q '#define NUMBER_OF_USB_PIPES 4' $(SRC)/usvc_config.h
	@LC_ALL=C grep -q '#define MAX_TILES (104)' $(SRC)/usvc_config.h
	@LC_ALL=C sed -e 's/MAX_BOOTLOADER_SIZE = 16384/MAX_BOOTLOADER_SIZE = 32768/' \
		$(UPSTREAM)/Device_Startup/samd21g18a_flash.ld > $(SRC)/samd21g18a_flash.ld
	@LC_ALL=C grep -q 'MAX_BOOTLOADER_SIZE = 32768' $(SRC)/samd21g18a_flash.ld
	@touch $@

# Upstream code is not warning-clean; that is not ours to fix.
$(BUILD)/obj/kernel/%.o: $(UPSTREAM)/usvc_kernel/%.c $(STAMP)
	$(CC) $(CFLAGS) -w -MMD -MP -c $(SRC)/usvc_kernel/$*.c -o $@

$(BUILD)/obj/startup_samd21.o: $(UPSTREAM)/Device_Startup/startup_samd21.c $(STAMP)
	$(CC) $(CFLAGS) -w -MMD -MP -c $< -o $@

$(BUILD)/obj/init.o: $(HUBTEST)/../init.c $(STAMP)
	$(CC) $(CFLAGS) -Wall -MMD -MP -c $< -o $@

$(BUILD)/obj/hubtest.o: $(HUBTEST)/hubtest.c $(STAMP)
	$(CC) $(CFLAGS) -Wall -MMD -MP -c $< -o $@

$(ELF): $(OBJS)
	$(CC) -o $@ $(OBJS) $(LDFLAGS) -Wl,-Map=$(BUILD)/hubtest.map

$(BIN): $(ELF)
	$(OBJCOPY) -O binary $< $@

$(LSS): $(ELF)
	$(OBJDUMP) -h -t -S $< > $@

clean:
	rm -rf $(BUILD)

-include $(OBJS:.o=.d)

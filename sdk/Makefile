# Build a uSVC game with arm-none-eabi-gcc.
#
#   make -C sdk GAME=../games/hello            build build/hello/hello.usc
#   make -C sdk GAME=../games/hello run        build, run headless, write a PNG
#   make -C sdk GAME=../games/hello play       build and play in a window
#   make -C sdk GAME=../games/hello EMULATOR=1 enable usvcDebugPrint
#   make -C sdk GAME=../games/shatterline assets   PNG artwork -> gen/*.c
#
# The kernel, start-up code and linker script come from the upstream template
# unless the game directory has its own usvc_kernel/ or Device_Startup/.

ifeq ($(GAME),)
$(error set GAME to the game source directory, e.g. GAME=../games/hello)
endif

SDK      := $(patsubst %/,%,$(dir $(abspath $(lastword $(MAKEFILE_LIST)))))
ROOT     := $(abspath $(SDK)/..)
GAME_DIR := $(abspath $(GAME))
# A game may set TITLE and other variables in its own game.mk.
-include $(GAME_DIR)/game.mk
NAME     ?= $(notdir $(GAME_DIR))
TITLE    ?= $(NAME)
# Emulator builds get their own directory, so that objects with the debug
# port compiled in can never end up in a package meant for hardware.
BUILD    ?= $(ROOT)/build/$(NAME)$(if $(EMULATOR),-emu)
UPSTREAM := $(ROOT)/reference/uSVC/software/uSVC_Template_Project
KERNEL   ?= $(if $(wildcard $(GAME_DIR)/usvc_kernel),$(GAME_DIR)/usvc_kernel,$(UPSTREAM)/usvc_kernel)
STARTUP  ?= $(if $(wildcard $(GAME_DIR)/Device_Startup),$(GAME_DIR)/Device_Startup,$(UPSTREAM)/Device_Startup)
FRAMES   ?= 120

CC      := arm-none-eabi-gcc
OBJCOPY := arm-none-eabi-objcopy
OBJDUMP := arm-none-eabi-objdump
SIZE    := arm-none-eabi-size

# Same flags as the upstream Atmel Studio projects.
ARCH    := -mthumb -mcpu=cortex-m0plus
DEFS    := -D__SAMD21E18A__ -DNDEBUG -DUSE_BOOTLOADER $(if $(EMULATOR),-DUSVC_EMULATOR)
# The kernel includes "../usvc_config.h", so it is copied into the build
# directory next to a copy of the game's configuration.
INCS    := -I$(BUILD) -I$(GAME_DIR) -I$(SDK)/include -I$(BUILD)/usvc_kernel/include -I$(SDK)/cmsis
CFLAGS  := -x c $(ARCH) $(DEFS) $(INCS) -O3 -ffunction-sections -mlong-calls -std=gnu99
LDFLAGS := $(ARCH) -Wl,--start-group -lm -Wl,--end-group -L$(STARTUP) -Wl,--gc-sections \
           -Tsamd21g18a_flash.ld -Xlinker --defsym=USE_BOOTLOADER=1 \
           --specs=nano.specs --specs=nosys.specs -Wl,--no-warn-rwx-segments

# Game sources, plus generated ones (see tools/gfx.py) in gen/.
GAME_SRCS    := $(wildcard $(GAME_DIR)/*.c)
GEN_SRCS     := $(wildcard $(GAME_DIR)/gen/*.c)
KERNEL_SRCS  := $(wildcard $(KERNEL)/*.c)
STARTUP_SRCS := $(wildcard $(STARTUP)/*.c)
OBJS := $(patsubst $(GAME_DIR)/%.c,$(BUILD)/obj/game/%.o,$(GAME_SRCS)) \
        $(patsubst $(GAME_DIR)/gen/%.c,$(BUILD)/obj/gen/%.o,$(GEN_SRCS)) \
        $(patsubst $(KERNEL)/%.c,$(BUILD)/obj/kernel/%.o,$(KERNEL_SRCS)) \
        $(patsubst $(STARTUP)/%.c,$(BUILD)/obj/startup/%.o,$(STARTUP_SRCS))

ELF := $(BUILD)/$(NAME).elf
BIN := $(BUILD)/$(NAME).bin
LSS := $(BUILD)/$(NAME).lss
USC := $(BUILD)/$(NAME).usc
STAMP := $(BUILD)/.kernel

.PHONY: all run play assets clean
all: $(USC) $(LSS)
	@$(SIZE) $(ELF)

$(STAMP): $(KERNEL_SRCS) $(wildcard $(KERNEL)/*.h) $(GAME_DIR)/usvc_config.h
	@mkdir -p $(BUILD)/obj/game $(BUILD)/obj/gen $(BUILD)/obj/kernel $(BUILD)/obj/startup
	@rsync -a --delete $(KERNEL)/ $(BUILD)/usvc_kernel/
	@cp $(GAME_DIR)/usvc_config.h $(BUILD)/usvc_config.h
	@touch $@

$(BUILD)/obj/game/%.o: $(GAME_DIR)/%.c $(STAMP)
	$(CC) $(CFLAGS) -Wall -MMD -MP -c $< -o $@

$(BUILD)/obj/gen/%.o: $(GAME_DIR)/gen/%.c $(STAMP)
	$(CC) $(CFLAGS) -Wall -MMD -MP -c $< -o $@

# Upstream kernel code is not warning-clean; that is not ours to fix.
$(BUILD)/obj/kernel/%.o: $(KERNEL)/%.c $(STAMP)
	$(CC) $(CFLAGS) -w -MMD -MP -c $(BUILD)/usvc_kernel/$*.c -o $@

$(BUILD)/obj/startup/%.o: $(STARTUP)/%.c $(STAMP)
	$(CC) $(CFLAGS) -w -MMD -MP -c $< -o $@

$(ELF): $(OBJS)
	$(CC) -o $@ $(OBJS) $(LDFLAGS) -Wl,-Map=$(BUILD)/$(NAME).map

$(BIN): $(ELF)
	$(OBJCOPY) -O binary $< $@

$(LSS): $(ELF)
	$(OBJDUMP) -h -S $< > $@

$(USC): $(BIN)
	cd $(ROOT)/tools && uv run usc.py pack $(BIN) $(USC) --title "$(TITLE)"

run: all
	cargo build --release --manifest-path $(ROOT)/Cargo.toml
	$(ROOT)/target/release/usvc $(USC) --headless --frames $(FRAMES) --lss $(LSS) \
		--png $(BUILD)/$(NAME).png $(RUNFLAGS)

play: all
	cargo build --release --manifest-path $(ROOT)/Cargo.toml
	$(ROOT)/target/release/usvc $(USC) --lss $(LSS) $(RUNFLAGS)

# Regenerate gen/*.c from the PNG artwork in the game's assets/ directory and
# the songs in its music/ directory.
assets:
	cd $(ROOT)/tools && uv run gfx.py tiles $(GAME_DIR)/assets/tiles.png \
		$(GAME_DIR)/assets/tiles.txt $(GAME_DIR)/gen/tiles
	cd $(ROOT)/tools && uv run gfx.py sprites $(GAME_DIR)/assets/sprites.png \
		$(GAME_DIR)/assets/frames.txt $(GAME_DIR)/gen/sprites
	@if ls $(GAME_DIR)/music/*.song >/dev/null 2>&1; then \
		cd $(ROOT)/tools && uv run song.py $(GAME_DIR)/gen/songs $(GAME_DIR)/music/*.song; fi

clean:
	rm -rf $(BUILD)

-include $(OBJS:.o=.d)

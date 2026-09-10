CC = aarch64-linux-gnu-gcc
AS = aarch64-linux-gnu-as
LD = aarch64-linux-gnu-ld
OBJCOPY = aarch64-linux-gnu-objcopy

QEMU = qemu-system-aarch64
BUILD = build
KERNEL = $(BUILD)/kernel8.img

# Переменная платформы по умолчанию: qemu (машина virt)
BOARD ?= qemu

CFLAGS = -Wall -Wextra -ffreestanding -nostdlib -mgeneral-regs-only -mcpu=cortex-a76 -g3 -Isrc
LDFLAGS = -T linker/kernel.ld -nostdlib

# -------------------------------------------------------------------------
# ИСПРАВЛЕННЫЙ ИНТЕЛЛЕКТУАЛЬНЫЙ ПОИСК (filter-out)
# -------------------------------------------------------------------------
SRCS_ALL_C = $(shell find . -type f -name "*.c" -not -path "*/$(BUILD)/*")

# Исключаем ненужный драйвер UART в зависимости от платформы
ifeq ($(BOARD),radxa)
    SRCS_C = $(filter-out %pl011.c, $(SRCS_ALL_C))
    CFLAGS += -DTARGET_RADXA
else
    SRCS_C = $(filter-out %rk16550.c, $(SRCS_ALL_C))
endif

SRCS_ASM = $(shell find . -type f \( -name "*.S" -o -name "*.s" \) -not -path "*/$(BUILD)/*")

OBJS_C   = $(patsubst %.c, $(BUILD)/%.o, $(notdir $(SRCS_C)))
OBJS_ASM = $(patsubst %.S, $(BUILD)/%.o, $(patsubst %.s, $(BUILD)/%.o, $(notdir $(SRCS_ASM))))

VPATH = $(sort $(dir $(SRCS_C) $(SRCS_ASM)))

all: $(KERNEL)

$(BUILD):
	mkdir -p $(BUILD)

$(BUILD)/%.o: %.S | $(BUILD)
	@echo "[AS] Компиляция: $< -> $@"
	$(CC) $(CFLAGS) -c $< -o $@

$(BUILD)/%.o: %.s | $(BUILD)
	@echo "[AS] Компиляция: $< -> $@"
	$(CC) $(CFLAGS) -c $< -o $@

$(BUILD)/%.o: %.c | $(BUILD)
	@echo "[CC] Компиляция: $< -> $@"
	$(CC) $(CFLAGS) -c $< -o $@

$(KERNEL): $(OBJS_ASM) $(OBJS_C)
	@echo "[LD] Линкуем файлы для платформы [$(BOARD)]: $^"
	$(LD) $(LDFLAGS) $^ -o $(BUILD)/kernel.elf
	$(OBJCOPY) -O binary $(BUILD)/kernel.elf $@
	@echo "Kernel built successfully!"

run:
	@$(MAKE) BOARD=qemu $(KERNEL)
	$(QEMU) -M virt,gic-version=3 -cpu cortex-a76 -m 16G -kernel $(KERNEL) -nographic

clean:
	rm -rf $(BUILD)

.PHONY: all run clean

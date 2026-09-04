CC = aarch64-linux-gnu-gcc
AS = aarch64-linux-gnu-as
LD = aarch64-linux-gnu-ld
OBJCOPY = aarch64-linux-gnu-objcopy
QEMU = qemu-system-aarch64

BUILD = build
KERNEL = $(BUILD)/kernel8.img

CFLAGS = -Wall -Wextra -ffreestanding -nostdlib -mgeneral-regs-only
LDFLAGS = -T linker/kernel.ld -nostdlib

all: $(KERNEL)

$(BUILD):
	mkdir -p $(BUILD)

$(BUILD)/boot.o: boot/boot.S | $(BUILD)
	$(CC) $(CFLAGS) -c $< -o $@

$(BUILD)/kernel.o: src/kernel.c | $(BUILD)
	$(CC) $(CFLAGS) -c $< -o $@

$(KERNEL): $(BUILD)/boot.o $(BUILD)/kernel.o
	$(LD) $(LDFLAGS) $^ -o $(BUILD)/kernel.elf
	$(OBJCOPY) -O binary $(BUILD)/kernel.elf $@
	@echo "Kernel built: $@"
	@ls -lh $@

run: $(KERNEL)
	$(QEMU) -M raspi3b -kernel $< -serial stdio

run64: $(KERNEL)
	$(QEMU) -M virt -cpu cortex-a57 -kernel $< -serial stdio -nographic

debug: $(KERNEL)
	$(QEMU) -M raspi3b -kernel $< -serial stdio -s -S &
	@echo "QEMU started. Connect with:"
	@echo "  gdb-multiarch -ex 'target remote :1234' $(BUILD)/kernel.elf"

clean:
	rm -rf $(BUILD)

.PHONY: all run run64 debug clean
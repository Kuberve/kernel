/* Kuberve Microkernel — ARM64 */
#include <stdint.h>
#include "ivt/interrupts.h"
#include "timer/timer.h"
#include "drivers/uart/uart.h"
#include "drivers/uart/printk.h"
#include "mm/mmu.h"

extern volatile uint64_t system_ticks;

void kernel_main(uint64_t dtb_address) {

    printk("\n==================================\n");
    printk("=== KUBERVE MICROKERNEL v0.1.0 ===\n");
    printk("==================================\n");
    printk("Architecture : ARMv8-A (AArch64)\n");
    
#ifdef TARGET_RADXA
    printk("Target Board : Radxa Rock 5 ITX (RK3588)\n");
#else
    printk("Target Board : QEMU Virtual Machine (virt)\n");
#endif

    init_mmu();
    printk("Memory : MMU Enabled. Data & Instruction Caches Active.\n");

    init_interrupts(); 
    printk("System : GICv3 and Architecture Timer Initialized.\n");

    printk("Boot Pointer : Device Tree Blob loaded at %p\n", (void*)dtb_address);
    printk("Kernel Status: %s (Core ID: %d)\n", "Running", 0);
    printk("Test Numbers : Decimal: %d, Hex: 0x%x\n", 123456, 0xABCDEF);

    printk("\nStatus : Running smoothly\n");
    printk("==================================\n"); 
    
    printk("System Timer : %d Hz (1 tick = %d ms)\n", 1000, 1000 / 1000);
    sleep(1000); // Задержка 1 секунда для демонстрации работы таймера
    printk("System Timer : %d ticks since boot\n", system_ticks);

    uint64_t last_tick = 0;
    while (1) {
        __asm__ volatile("wfi");
    }
}

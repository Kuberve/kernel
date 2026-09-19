#include <stdint.h>
#include "ivt/interrupts.h"
#include "timer/timer.h"
#include "drivers/uart/uart.h"
#include "drivers/uart/printk.h"

volatile uint64_t kuberve_master_ticks = 0;

extern void uart_putc(char c);
extern void uart_puts(const char *s);

void kernel_main(uint64_t dtb_address) {
    (void)dtb_address;

    printk("\n==================================\n");
    printk("=== KUBERVE MICROKERNEL v0.1.0 ===\n");
    printk("==================================\n");
    printk("Architecture : ARMv8-A (AArch64)\n");
    printk("Target Board : QEMU Virtual Machine (virt)\n");
    printk("Timer : Virtual (CNTV, IRQ 27)\n");
    printk("MMU : DISABLED\n");

    init_interrupts(); 
    uart_puts("[INT] Interrupts initialized\n");
    
    printk("==================================\n");
    
    uart_puts("[MAIN] Entering loop...\n");

    uint64_t iteration = 0;
    uint64_t prev_ticks = 0;
    
    while (1) {
        iteration++;
        
        if (iteration % 1000000 == 0) {
            uart_putc('.');
        }
        
        if (kuberve_master_ticks != prev_ticks) {
            uart_putc('X'); // Сигнал из main loop
            prev_ticks = kuberve_master_ticks;
        }
    }
}
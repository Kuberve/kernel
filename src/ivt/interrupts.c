#include <stdint.h>
#include "../drivers/uart/uart.h"
#include "../drivers/uart/printk.h"
#include "../timer/timer.h"

extern volatile uint64_t kuberve_master_ticks;

extern void uart_putc(char c);
extern void uart_puts(const char *s);

// Добавь эту функцию
static void print_hex64(uint64_t val) {
    const char hex[] = "0123456789abcdef";
    for (int i = 0; i < 16; i++) {
        uart_putc(hex[(val >> (60 - i * 4)) & 0xF]);
    }
}

#define GICD_BASE 0x08000000ULL
#define GICR_BASE 0x080A0000ULL

#define GICD_CTLR ((volatile uint32_t *)(GICD_BASE + 0x000))
#define GICR_WAKER ((volatile uint32_t *)(GICR_BASE + 0x0014))
#define GICR_IGROUPR0 ((volatile uint32_t *)(GICR_BASE + 0x0080))
#define GICR_ISENABLER0 ((volatile uint32_t *)(GICR_BASE + 0x0100))
#define GICR_IPRIORITYR0 ((volatile uint32_t *)(GICR_BASE + 0x0400))

static void gicv3_init(void) {
    asm volatile("msr S3_0_C12_C12_5, %0" : : "r"(1 | 8)); 
    asm volatile("isb");

    *GICR_WAKER &= ~(1 << 1);          
    while (*GICR_WAKER & (1 << 2)) {}  

    *GICD_CTLR = 3; 
    asm volatile("isb");

    // ВИРТУАЛЬНЫЙ таймер: PPI 11 = IRQ 27
    uint32_t ppi_number = 11;
    *GICR_IGROUPR0 |= (1 << ppi_number);
    *(GICR_IPRIORITYR0 + (ppi_number / 4)) = 0;
    *GICR_ISENABLER0 = (1 << ppi_number);

    asm volatile("msr S3_0_C4_C6_0, %0" : : "r"((uint64_t)0xFF));
    asm volatile("msr S3_0_C12_C12_3, %0" : : "r"((uint64_t)0));
    asm volatile("msr S3_0_C12_C12_7, %0" : : "r"((uint64_t)1));
    asm volatile("isb");
}

void c_sync_handler(void) {
    uint64_t esr, elr, far;
    asm volatile("mrs %0, esr_el1" : "=r"(esr));
    asm volatile("mrs %0, elr_el1" : "=r"(elr));
    asm volatile("mrs %0, far_el1" : "=r"(far));
    
    uart_puts("\n[!!!] SYNC EXCEPTION [!!!]\n");
    uart_puts("ESR=");
    print_hex64(esr);
    uart_puts(" ELR=");
    print_hex64(elr);
    uart_puts(" FAR=");
    print_hex64(far);
    uart_puts("\n");
    
    while (1) { __asm__ volatile("wfi"); }
}

void c_irq_handler(void) {
    uart_putc('I'); // Сигнал входа в обработчик
    
    uint64_t iar;
    asm volatile("mrs %0, S3_0_C12_C12_0" : "=r"(iar)); 
    uint32_t irq_id = iar & 0xFFFFFF; 

    if (irq_id >= 1020) return;

    if (irq_id == 27) {  // ВИРТУАЛЬНЫЙ таймер = IRQ 27
        uart_putc('T');
        core_timer_handler(&kuberve_master_ticks);
    }

    asm volatile("msr S3_0_C12_C12_1, %0" : : "r"(iar));
    asm volatile("isb"); 
}

void init_interrupts(void) {
    extern void exception_vector_table(void);

    asm volatile("msr vbar_el1, %0" : : "r"(exception_vector_table));
    asm volatile("isb");

    gicv3_init();
    core_timer_init(1000, &kuberve_master_ticks);

    uint64_t daif_val;
    asm volatile("mrs %0, daif" : "=r"(daif_val));
    daif_val &= ~(1ULL << 7);
    asm volatile("msr daif, %0" : : "r"(daif_val));
    asm volatile("isb");
}
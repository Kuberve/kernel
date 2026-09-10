#include <stdint.h>
#include "timer/timer.h"
#include "drivers/uart/printk.h"

#define GIC_DIST_BASE           0xFE600000 
#define GIC_REDIST_BASE         0xFE660000 

#define GICD_CTLR               ((volatile uint32_t*)(GIC_DIST_BASE + 0x0000))
#define GICD_TYPER              ((volatile uint32_t*)(GIC_DIST_BASE + 0x0004))

#define GICR_SGI_BASE           (GIC_REDIST_BASE + 0x10000)
#define GICR_ISENABLER0         ((volatile uint32_t*)(GICR_SGI_BASE + 0x0100))
#define GICR_ICENABLER0         ((volatile uint32_t*)(GICR_SGI_BASE + 0x0180))
#define GICR_IGROUPR0           ((volatile uint32_t*)(GICR_SGI_BASE + 0x0080))
#define GICR_IGRPMODR0          ((volatile uint32_t*)(GICR_SGI_BASE + 0x0D00))

#define GICR_WAKER              ((volatile uint32_t*)(GIC_REDIST_BASE + 0x0014))

static void gicv3_init(void) {
    asm volatile("msr S3_0_C12_C12_5, %0" : : "r"(1 | 8)); 
    asm volatile("isb");

    *GICR_WAKER &= ~(1 << 1);          
    while (*GICR_WAKER & (1 << 2)) {}  

    *GICD_CTLR = 3; 
    asm volatile("isb");

    *GICR_IGROUPR0  |= (1 << 30);
    *GICR_IGRPMODR0 &= ~(1 << 30);
    *GICR_ISENABLER0 = (1 << 30);

    asm volatile("msr S3_0_C4_C6_0, %0" : : "r"((uint64_t)0xFF));
    asm volatile("msr S3_0_C12_C12_7, %0" : : "r"((uint64_t)1));
    asm volatile("isb");
}

void c_sync_handler(void) {
    uint64_t esr, elr, far;

    asm volatile("mrs %0, esr_el1" : "=r"(esr)); 
    asm volatile("mrs %0, elr_el1" : "=r"(elr)); 
    asm volatile("mrs %0, far_el1" : "=r"(far)); 

    uint32_t ec = (esr >> 26) & 0x3F; 

    printk("PANIC: Sync Exception! EC: 0x%x, PC: 0x%llx, FAR: 0x%llx\n", ec, elr, far);

    while(1) {
        asm volatile("wfi"); 
    }
}

// Главный диспетчер аппаратных прерываний (IRQ)
void c_irq_handler(void) {
    uint64_t iar;

    asm volatile("mrs %0, S3_0_C12_C12_0" : "=r"(iar)); 

    uint32_t irq_id = iar & 0xFFFFFF; 

    // Защита от фантомных прерываний GICv3 (Spurious Interrupts)
    // ID 1023 означает, что ожидающих прерываний нет. Возвращаемся.
    if (irq_id >= 1020) {
        return;
    }

    if (irq_id == 30) {
        core_timer_handler(); 
        // os_schedule_tick();
    }
    else if (irq_id == 153) {
        // rk3588_uart_interrupt_handler();
    }
    else {
        // Опционально: можно раскомментировать для отладки "заблудившихся" прерываний
        // printk("WARNING: Unhandled IRQ %d\n", irq_id);
    }

    // Сигнализируем об окончании обработки
    asm volatile("msr S3_0_C12_C12_1, %0" : : "r"(iar));
    asm volatile("isb"); 
}

void init_interrupts(void) {
    extern void exception_vector_table(void);

    asm volatile("msr vbar_el1, %0" : : "r"(exception_vector_table));
    asm volatile("isb");

    gicv3_init();

    // ИЗМЕНЕНО: Запускаем таймер на частоте 1000 Гц (шаг 1 мс). 
    // Теперь 1 system_tick = 1 миллисекунда. Это идеальное соотношение для функции sleep(ms).
    core_timer_init(1000);

    asm volatile("msr daifclr, #2"); 
}
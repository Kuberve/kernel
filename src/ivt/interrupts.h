#ifndef INTERRUPTS_H
#define INTERRUPTS_H

#include <stdint.h>

void init_interrupts(void);

extern void exception_vector_table(void);

static inline void enable_irq(void) {
    asm volatile("msr daifclr, #2" : : : "memory");
}

static inline void disable_irq(void) {
    asm volatile("msr daifset, #2" : : : "memory");
}

static inline uint64_t save_irq_disable(void) {
    uint64_t state;
    asm volatile("mrs %0, daif" : "=r"(state));
    disable_irq();
    return state;
}

static inline void restore_irq(uint64_t state) {
    asm volatile("msr daif, %0" : : "r"(state) : "memory");
}

#endif
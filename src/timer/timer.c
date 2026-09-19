#include "timer.h"
#include "drivers/uart/uart.h"

uint64_t timer_interval_ticks = 0;
uint32_t timer_hz = 1000;

void core_timer_init(uint32_t hz, volatile uint64_t *ticks_ptr) {
    (void)ticks_ptr;
    timer_hz = hz;
    uint64_t freq;
    
    asm volatile("mrs %0, cntfrq_el0" : "=r"(freq));
    timer_interval_ticks = freq / hz;
    
    uint64_t next_trigger;
    asm volatile("mrs %0, cntvct_el0" : "=r"(next_trigger));
    next_trigger += timer_interval_ticks;
    
    // ВИРТУАЛЬНЫЙ таймер
    asm volatile("msr cntv_cval_el0, %0" : : "r"(next_trigger));
    asm volatile("msr cntv_ctl_el0, %0" : : "r"((uint64_t)1));
    asm volatile("isb");
}

__attribute__((noinline))
void core_timer_handler(volatile uint64_t *ticks_ptr) {
    (*ticks_ptr)++;
    
    uint64_t cval;
    asm volatile("mrs %0, cntv_cval_el0" : "=r"(cval));
    cval += timer_interval_ticks;
    asm volatile("msr cntv_cval_el0, %0" : : "r"(cval));
}
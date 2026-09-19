#ifndef TIMER_H
#define TIMER_H

#include <stdint.h>

extern volatile uint64_t kuberve_master_ticks;
extern uint64_t timer_interval_ticks;

void core_timer_init(uint32_t hz, volatile uint64_t *ticks_ptr);
void core_timer_handler(volatile uint64_t *ticks_ptr);

#endif // TIMER_H
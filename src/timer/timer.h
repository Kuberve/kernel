#ifndef TIMER_H
#define TIMER_H

#include <stdint.h>

// Initialization of the processor’s physical timer at the specified frequency (Hz)
void core_timer_init(uint32_t hz);

// Reset the timer to the next interval (called within the IRQ handler)
void core_timer_handler(void);

// Get the current number of processor ticks since the system was powered on (for delays)
uint64_t core_timer_get_ticks(void);

// Sleep for a specified number of milliseconds (ms)
void sleep(uint32_t ms);

#endif // TIMER_H

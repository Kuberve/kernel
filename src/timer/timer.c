#include "timer.h"

static uint32_t timer_hz = 1000;
static uint64_t timer_interval_ticks = 0;
volatile uint64_t system_ticks = 0;

/* 
 * ВАЖНО: Если ты используешь QEMU -M virt, ядро стартует в EL2.
 * Эта функция должна быть вызвана ПЕРЕД переходом в EL1, 
 * чтобы разрешить EL1 использовать физический таймер.
 * Если ты используешь -M raspi3b, она стартует сразу в EL1, и это можно пропустить.
 */
void el2_enable_timer_access(void) {
    uint64_t hcr;
    asm volatile("mrs %0, hcr_el2" : "=r"(hcr));
    hcr |= (1 << 0);  // HCR_EL2.TGE = 0 (обычный режим)
    hcr |= (1 << 4);  // HCR_EL2.FMO = 1 (перехватывать FIQ в EL2, если нужно)
    hcr |= (1 << 5);  // HCR_EL2.IMO = 1 (перехватывать IRQ в EL2, если нужно)
    asm volatile("msr hcr_el2, %0" : : "r"(hcr));

    // Разрешаем EL1 доступ к физическому таймеру (CNTHCTL_EL2)
    asm volatile("msr cnthctl_el2, %0" : : "r"(3)); // Бит 0 (EL1PCEN) и Бит 1 (EL1PCTEN) = 1
    
    asm volatile("isb");
}

void core_timer_init(uint32_t hz) {
    timer_hz = hz2;
    uint64_t freq;
    
    asm volatile("mrs %0, cntfrq_el0" : "=r"(freq));
    timer_interval_ticks = freq / hz;
    
    uint64_t next_trigger;
    asm volatile("mrs %0, cntpct_el0" : "=r"(next_trigger));
    next_trigger += timer_interval_ticks;
    
    asm volatile("msr cntp_cval_el0, %0" : : "r"(next_trigger));
    
    // Включаем таймер: Бит 0 = 1 (Enable), Бит 1 = 0 (Interrupt NOT masked)
    asm volatile("msr cntp_ctl_el0, %0" : : "r"((uint64_t)1));
    asm volatile("isb");
}

/* Функция для глобального разрешения прерываний (IRQ) */
void enable_irq(void) {
    // Очищаем бит I (IRQ mask) в регистре DAIF
    // 2 = 0b0010, что соответствует очистке бита I
    asm volatile("msr daifclr, #2");
    asm volatile("isb");
}

void core_timer_handler(void) {
    system_ticks++;
    
    uint64_t cval;
    asm volatile("mrs %0, cntp_cval_el0" : "=r"(cval));
    cval += timer_interval_ticks;
    asm volatile("msr cntp_cval_el0, %0" : : "r"(cval));
}

uint64_t core_timer_get_ticks(void) {
    uint64_t ticks;
    asm volatile("mrs %0, cntpct_el0" : "=r"(ticks));
    return ticks;
}

void sleep(uint32_t ms) {
    uint64_t target_ticks = system_ticks + ms;
    
    while (system_ticks < target_ticks) {
        // wfi усыпляет ядро до следующего прерывания.
        // Если прерывания выключены, ядро уснёт навсегда!
        asm volatile("wfi"); 
    }
}
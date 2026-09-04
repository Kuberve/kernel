/* Kuberve Microkernel — ARM64 */
#include <stdint.h>

/* UART0 на Raspberry Pi 3/4 (PL011) */
#define UART0_BASE 0x3F201000
#define UART0_DR   *((volatile uint32_t *)(UART0_BASE))
#define UART0_FR   *((volatile uint32_t *)(UART0_BASE + 0x18))
#define UART0_FR_TX_FULL 0x20

/* Отправка символа */
void uart_putc(char c) {
    while (UART0_FR & UART0_FR_TX_FULL);
    UART0_DR = c;
}

/* Отправка строки */
void uart_puts(const char *s) {
    while (*s) {
        if (*s == '\n') uart_putc('\r');
        uart_putc(*s++);
    }
}

/* Точка входа из bootloader */
void kernel_main(void) {
    uart_puts("\n=== KUBERVE MICROKERNEL v0.1.0 ===\n");
    uart_puts("Architecture: ARMv8-A (AArch64)\n");
    uart_puts("Status: Running\n");
    uart_puts("=================================\n");
    
    /* Бесконечный цикл */
    while (1) {
        __asm__ volatile("wfi");
    }
}
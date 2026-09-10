#include <stdint.h>

#define UART_BASE      0xFEB50000
#define UART_DR        *((volatile uint32_t *)(UART_BASE + 0x00))
#define UART_LSR       *((volatile uint32_t *)(UART_BASE + 0x14))
#define UART_LSR_THRE  (1 << 5)

void uart_putc(char c) {
    while (!(UART_LSR & UART_LSR_THRE));
    UART_DR = c;
}

void uart_puts(const char *s) {
    while (*s) {
        if (*s == '\n') uart_putc('\r');
        uart_putc(*s++);
    }
}

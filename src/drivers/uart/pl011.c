#include <stdint.h>

#define UART_BASE    0x09000000
#define UART_DR      *((volatile uint32_t *)(UART_BASE + 0x00))
#define UART_FR      *((volatile uint32_t *)(UART_BASE + 0x18))
#define UART_FR_TXFF (1 << 5)

void uart_putc(char c) {
    while (UART_FR & UART_FR_TXFF);
    UART_DR = c;
}

void uart_puts(const char *s) {
    while (*s) {
        if (*s == '\n') uart_putc('\r');
        uart_putc(*s++);
    }
}

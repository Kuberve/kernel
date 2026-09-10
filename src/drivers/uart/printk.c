#include <stdint.h>
#include "drivers/uart/uart.h"
#include "drivers/uart/printk.h"

// Вспомогательная функция для перевода чисел в строку (для %d и %x)
static void itoa(uint64_t value, char *str, int base, int is_signed) {
    char *rc;
    char *ptr;
    char *low;
    
    // Набор символов для шестнадцатеричной системы
    const char *digits = "0123456789abcdef";

    // Обработка знака для десятичных чисел
    if (is_signed && base == 10 && (int64_t)value < 0) {
        *str++ = '-';
        value = -(int64_t)value;
    }

    rc = str;
    ptr = str;

    // Выделяем цифры в обратном порядке
    do {
        *ptr++ = digits[value % base];
        value /= base;
    } while (value);

    *ptr = '\0';
    low = rc;
    ptr--;

    // Разворачиваем строку в правильный порядок
    while (low < ptr) {
        char tmp = *low;
        *low++ = *ptr;
        *ptr-- = tmp;
    }
}

void printk(const char *fmt, ...) {
    // Используем встроенные механизмы компилятора для работы с переменным числом аргументов
    __builtin_va_list args;
    __builtin_va_start(args, fmt);

    char buf[64];

    for (const char *p = fmt; *p != '\0'; p++) {
        if (*p != '%') {
            if (*p == '\n') uart_putc('\r');
            uart_putc(*p);
            continue;
        }

        p++; // Пропускаем '%'
        
        switch (*p) {
            case 'c': {
                char c = (char)__builtin_va_arg(args, int);
                uart_putc(c);
                break;
            }
            case 's': {
                char *s = __builtin_va_arg(args, char *);
                if (!s) s = "(null)";
                uart_puts(s);
                break;
            }
            case 'd': {
                int64_t d = __builtin_va_arg(args, int64_t);
                itoa(d, buf, 10, 1);
                uart_puts(buf);
                break;
            }
            case 'x': {
                uint64_t x = __builtin_va_arg(args, uint64_t);
                itoa(x, buf, 16, 0);
                uart_puts(buf);
                break;
            }
            case 'p': {
                uint64_t p_val = (uint64_t)__builtin_va_arg(args, void *);
                uart_puts("0x");
                itoa(p_val, buf, 16, 0);
                uart_puts(buf);
                break;
            }
            case '%': {
                uart_putc('%');
                break;
            }
            default: {
                // Если спецификатор неизвестен, просто выводим как есть
                uart_putc('%');
                uart_putc(*p);
                break;
            }
        }
    }

    __builtin_va_end(args);
}

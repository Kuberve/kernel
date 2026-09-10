#ifndef MMU_H
#define MMU_H

#include <stdint.h>

// Флаги доступа к страницам памяти (Атрибуты дескрипторов)
#define MMU_DESCRIPTOR_VALID      (1ULL << 0)
#define MMU_DESCRIPTOR_BLOCK      (0ULL << 1) // Для L1/L2 таблиц: указывает на блок
#define MMU_DESCRIPTOR_TABLE      (1ULL << 1) // Указывает на следующую таблицу страниц
#define MMU_ATTR_INDX_NORMAL      (0ULL << 2) // Индекс в таблице MAIR (Обычная память с кэшем)
#define MMU_ATTR_INDX_DEVICE      (1ULL << 2) // Индекс в таблице MAIR (Периферия/Регистры)
#define MMU_ACCESS_KERNEL_RW      (1ULL << 6) // AP[1] = 0, AP[0] = 1 (Ядро: Чтение/Запись)
#define MMU_ACCESS_INNER_SHARE    (3ULL << 8) // Доступно всем ядрам процессора
#define MMU_ACCESS_AF             (1ULL << 10) // Access Flag (Должен быть 1, иначе будет fault)

// Маски для сборки дескриптора блока 2 МБ
#define MMU_BLOCK_2MB_NORMAL   (MMU_DESCRIPTOR_VALID | MMU_DESCRIPTOR_BLOCK | \
                                MMU_ATTR_INDX_NORMAL | MMU_ACCESS_KERNEL_RW | \
                                MMU_ACCESS_INNER_SHARE | MMU_ACCESS_AF)

#define MMU_BLOCK_2MB_DEVICE   (MMU_DESCRIPTOR_VALID | MMU_DESCRIPTOR_BLOCK | \
                                MMU_ATTR_INDX_DEVICE | MMU_ACCESS_KERNEL_RW | \
                                MMU_ACCESS_AF)

// Инициализирует таблицы страниц и включает MMU.
void init_mmu(void);

#endif // MMU_H

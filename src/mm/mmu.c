#include "mm/mmu.h"
#include "drivers/uart/printk.h"

// --- Явное определение флагов для надежности (чтобы точно был Access Flag и нужный тип) ---
#define MMU_DESC_VALID       (1ULL << 0)
#define MMU_DESC_TABLE       (1ULL << 1) // Бит 1 равен 1 -> таблица (Valid + Table = 0x3)
#define MMU_DESC_BLOCK       (0ULL << 1) // Бит 1 равен 0 -> блок L1/L2 (Valid + Block = 0x1)

#define MMU_ACCESS_FLAG      (1ULL << 10) // Обязательный флаг! Без него будет Access Flag Fault

// Индексы в регистре MAIR_EL1
#define MMU_ATTR_NORMAL_IDX  (0ULL << 2)  // Индекс 0
#define MMU_ATTR_DEVICE_IDX  (1ULL << 2)  // Индекс 1

// Inner Shareable (для кэшируемой памяти)
#define MMU_SH_INNER         (3ULL << 8)

// Готовые макросы блоков
#define MMU_BLOCK_2MB_NORMAL (MMU_DESC_VALID | MMU_DESC_BLOCK | MMU_ACCESS_FLAG | MMU_ATTR_NORMAL_IDX | MMU_SH_INNER)
#define MMU_BLOCK_2MB_DEVICE (MMU_DESC_VALID | MMU_DESC_BLOCK | MMU_ACCESS_FLAG | MMU_ATTR_DEVICE_IDX)

// Макрос перевода виртуального адреса в физический (для MMU).
// Сейчас ядро работает по реальным физическим адресам (1:1), поэтому преобразование прямое.
#define VIRT_TO_PHYS(x)      ((uint64_t)(x))
// -----------------------------------------------------------------------------------------


// Массивы таблиц по 512 элементов (ровно по 4096 байт каждый)
__attribute__((aligned(4096))) static uint64_t mmu_l0[512];
__attribute__((aligned(4096))) static uint64_t mmu_l1[512];
__attribute__((aligned(4096))) static uint64_t mmu_l2_kernel[512]; // 0.0 ГБ - 1.0 ГБ (MMIO)
__attribute__((aligned(4096))) static uint64_t mmu_l2_virt[512];   // 1.0 ГБ - 2.0 ГБ (RAM)

void init_mmu(void) {
    // 1. Очищаем таблицы страниц
    for (int i = 0; i < 512; i++) {
        mmu_l0[i] = 0;
        mmu_l1[i] = 0;
        mmu_l2_kernel[i] = 0;
        mmu_l2_virt[i] = 0;
    }

    // 2. Связываем L0[0] -> L1 (передаем ФИЗИЧЕСКИЙ адрес таблицы)
    mmu_l0[0] = VIRT_TO_PHYS(mmu_l1) | MMU_DESC_VALID | MMU_DESC_TABLE;

    // 3. Размечаем L1 таблицу
    mmu_l1[0] = VIRT_TO_PHYS(mmu_l2_kernel) | MMU_DESC_VALID | MMU_DESC_TABLE; // 0-1 ГБ
    mmu_l1[1] = VIRT_TO_PHYS(mmu_l2_virt) | MMU_DESC_VALID | MMU_DESC_TABLE;   // 1-2 ГБ
    
    // ДОБАВИТЬ: Прямой маппинг 3.0 ГБ - 4.0 ГБ (1 GB Block) для GIC, PCIe, USB
    // MMU_BLOCK_1GB_DEVICE = MMU_DESC_VALID | (0<<1) | MMU_ACCESS_FLAG | MMU_ATTR_DEVICE_IDX
    mmu_l1[3] = 0xC0000000 | 1 | (1<<10) | (1<<2); // Упрощенно: Block, AF=1, Device

    // 4. Заполняем первую L2 таблицу (0.0 ГБ - 1.0 ГБ)
    // В QEMU virt всё до 0x40000000 - это устройства (PCIe, UART, GIC, Flash).
    for (uint64_t i = 0; i < 512; i++) {
        uint64_t phys_addr = i * 2 * 1024 * 1024; // Шаг 2 МБ
        mmu_l2_kernel[i] = phys_addr | MMU_BLOCK_2MB_DEVICE; // ВЕСЬ гигабайт - Device
    }

    // 5. Заполняем вторую L2 таблицу (1.0 ГБ - 2.0 ГБ)
    // Здесь лежит RAM (0x40000000 = ровно 1 ГБ).
    for (uint64_t i = 0; i < 512; i++) {
        uint64_t phys_addr = (1ULL * 1024 * 1024 * 1024) + (i * 2 * 1024 * 1024);
        mmu_l2_virt[i] = phys_addr | MMU_BLOCK_2MB_NORMAL; 
    }

    // 6. НАСТРОЙКА СИСТЕМНЫХ РЕГИСТРОВ ПРОЦЕССОРА
    uint64_t mair = (0xFFULL << 0) | (0x04ULL << 8); // Индекс 0: Normal WB/WA, Индекс 1: Device-nGnRE
    asm volatile("msr mair_el1, %0" : : "r"(mair));

    // Настройка TCR_EL1: T0SZ=16 (48-бит адреса), 4КБ, IPS=48-бит (для поддержки ОЗУ > 3ГБ)
    uint64_t tcr = (16ULL << 0)  | // T0SZ = 16
                   (3ULL << 12)  | // SH0 (Inner Shareable)
                   (1ULL << 8)   | // ORGN0 (Normal WB WA)
                   (1ULL << 10)  | // IRGN0 (Normal WB WA)
                   (0ULL << 14)  | // TG0 (4KB granule)
                   (5ULL << 32);   // IPS = 5 (48-bit physical address)
    asm volatile("msr tcr_el1, %0" : : "r"(tcr));

    // БАРЬЕРЫ: Гарантируем, что таблицы физически записаны в память
    asm volatile("dsb ishst");

    // Передаем физический адрес корня (L0)
    asm volatile("msr ttbr0_el1, %0" : : "r"(VIRT_TO_PHYS(mmu_l0)));
    asm volatile("isb");

    // Обязательный сброс закэшированного мусора в TLB перед включением
    asm volatile("tlbi vmalle1"); 
    asm volatile("dsb sy");
    asm volatile("isb");

    // 7. ЗАПУСК MMU
    uint64_t sctlr;
    asm volatile("mrs %0, sctlr_el1" : "=r"(sctlr));
    sctlr |= (1ULL << 0) | (1ULL << 2) | (1ULL << 12); // M=1, C=1, I=1
    asm volatile("msr sctlr_el1, %0" : : "r"(sctlr));
    
    // Барьер инструкций: перестраиваем конвейер под новые виртуальные адреса
    asm volatile("isb"); 
}
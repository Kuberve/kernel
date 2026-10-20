use core::arch::asm;

const PT_LOAD: u32 = 1;

fn read_u16(data: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([data[off], data[off + 1]])
}

fn read_u32(data: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([data[off], data[off+1], data[off+2], data[off+3]])
}

fn read_u64(data: &[u8], off: usize) -> u64 {
    let mut b = [0u8; 8];
    b.copy_from_slice(&data[off..off+8]);
    u64::from_le_bytes(b)
}

/// Загружает ELF в память по его vaddr'ам. Возвращает entry point.
/// Предполагает, что vaddr попадают в identity-mapped RAM (как у нас).
pub unsafe fn load_elf(data: &[u8]) -> u64 {
    if data.len() < 64 {
        panic!("ELF too small: {} bytes", data.len());
    }
    if &data[0..4] != b"\x7fELF" {
        panic!("not an ELF: magic {:x?}", &data[0..4]);
    }
    if data[4] != 2 {
        panic!("not ELF64");
    }
    if data[5] != 1 {
        panic!("not little-endian");
    }

    let e_entry = read_u64(data, 24);
    let e_phoff = read_u64(data, 32);
    let e_phentsize = read_u16(data, 54);
    let e_phnum = read_u16(data, 56);

    crate::println!("ELF: entry={:#x}, phoff={}, phnum={}", e_entry, e_phoff, e_phnum);

    let mut max_end: u64 = 0;

    for i in 0..e_phnum {
        let off = e_phoff as usize + i as usize * e_phentsize as usize;
        let p_type = read_u32(data, off);
        if p_type != PT_LOAD {
            continue;
        }
        let p_offset = read_u64(data, off + 8);
        let p_vaddr  = read_u64(data, off + 16);
        let p_filesz = read_u64(data, off + 32);
        let p_memsz  = read_u64(data, off + 40);

        crate::println!("  LOAD: vaddr={:#x} filesz={:#x} memsz={:#x}",
                        p_vaddr, p_filesz, p_memsz);

        // Копируем байты из файла в память
        if (p_offset + p_filesz) as usize > data.len() {
            crate::println!("load_elf: LOAD segment out of bounds: offset={:#x} filesz={:#x} data.len={}",
                            p_offset, p_filesz, data.len());
            panic!("ELF truncated");
        }
        let src = data.as_ptr().add(p_offset as usize);
        let dst = p_vaddr as *mut u8;
        core::ptr::copy_nonoverlapping(src, dst, p_filesz as usize);

        // Обнуляем BSS (обычно memsz == filesz, но на всякий)
        if p_memsz > p_filesz {
            let bss_start = dst.add(p_filesz as usize);
            core::ptr::write_bytes(bss_start, 0, (p_memsz - p_filesz) as usize);
        }

        let seg_end = p_vaddr + p_memsz;
        if seg_end > max_end {
            max_end = seg_end;
        }
    }

    // Cache maintenance: сбросить D-cache на только что записанный код,
    // инвалидировать I-cache, барьер. Иначе EL0 может увидеть старый
    // (пустой) кэш инструкций вместо свежезагруженного кода.
    let mut addr = e_entry & !0x3F;   // выровнять на cache line
    while addr < max_end {
        asm!("dc cvau, {}", in(reg) addr);
        asm!("ic ivau, {}", in(reg) addr);
        addr += 64;
    }
    asm!("dsb ish");
    asm!("isb");

    e_entry
}
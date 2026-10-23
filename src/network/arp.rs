use crate::println;

pub const CACHE_SIZE: usize = 16;

#[derive(Clone, Copy)]
pub struct ArpEntry {
    pub ip: [u8; 4],
    pub mac: [u8; 6],
    pub valid: bool,
}

const EMPTY_ENTRY: ArpEntry = ArpEntry {
    ip: [0; 4],
    mac: [0; 6],
    valid: false,
};

static mut CACHE: [ArpEntry; CACHE_SIZE] = [EMPTY_ENTRY; CACHE_SIZE];

pub fn init() {
    unsafe {
        for i in 0..CACHE_SIZE {
            CACHE[i] = EMPTY_ENTRY;
        }
    }
}

/// Сохранить IP → MAC. Если IP уже есть — обновляем. Иначе — в первый свободный.
pub unsafe fn cache_insert(ip: [u8; 4], mac: [u8; 6]) {
    // Update existing
    for i in 0..CACHE_SIZE {
        if CACHE[i].valid && CACHE[i].ip == ip {
            CACHE[i].mac = mac;
            println!("arp: updated {} -> {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                     fmt_ip(&ip), mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
            return;
        }
    }
    // Find free slot
    for i in 0..CACHE_SIZE {
        if !CACHE[i].valid {
            CACHE[i] = ArpEntry { ip, mac, valid: true };
            println!("arp: cached {} -> {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                     fmt_ip(&ip), mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
            return;
        }
    }
    // Full — overwrite slot 0 (simple LRU-less)
    CACHE[0] = ArpEntry { ip, mac, valid: true };
    println!("arp: cache full, replaced slot 0 with {}", fmt_ip(&ip));
}

/// Найти MAC по IP. Возвращает `None`, если нет.
pub unsafe fn lookup(ip: [u8; 4]) -> Option<[u8; 6]> {
    for i in 0..CACHE_SIZE {
        if CACHE[i].valid && CACHE[i].ip == ip {
            return Some(CACHE[i].mac);
        }
    }
    None
}

/// Парсинг ARP-фрейма (frame после virtio_net_hdr). Возвращает (sender_ip, sender_mac, op).
/// Frame = Ethernet header (14) + ARP payload.
pub fn parse_arp(frame: &[u8]) -> Option<([u8; 4], [u8; 6], u16)> {
    if frame.len() < 14 + 28 { return None; }   // Ethernet + ARP
    if frame[12] != 0x08 || frame[13] != 0x06 { return None; }   // EtherType = ARP

    let arp = &frame[14..];
    // htype=arp[0..2], ptype=arp[2..4], hlen=arp[4], plen=arp[5]
    // op=arp[6..8], sha=arp[8..14], spa=arp[14..18], tha=arp[18..24], tpa=arp[24..28]
    if arp[4] != 6 || arp[5] != 4 { return None; }   // hlen=6, plen=4

    let op = ((arp[6] as u16) << 8) | arp[7] as u16;

    let mut sha = [0u8; 6];
    sha.copy_from_slice(&arp[8..14]);

    let mut spa = [0u8; 4];
    spa.copy_from_slice(&arp[14..18]);

    Some((spa, sha, op))
}

fn fmt_ip(ip: &[u8; 4]) -> FmtIp {
    FmtIp(*ip)
}

struct FmtIp([u8; 4]);
impl core::fmt::Display for FmtIp {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{}.{}.{}.{}", self.0[0], self.0[1], self.0[2], self.0[3])
    }
}
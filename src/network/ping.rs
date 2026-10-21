use crate::println;
use crate::device::{virtio_net, arp};

pub const OUR_IP: [u8; 4] = [10, 0, 2, 15];

const PING_ID: u16 = 0x1234;
static mut PING_SEQ: u16 = 0;
static mut SEND_TIME: u64 = 0;
static mut REPLY_RECEIVED: bool = false;

fn read_cntpct() -> u64 {
    let v: u64;
    unsafe { core::arch::asm!("mrs {}, CNTPCT_EL0", out(reg) v) };
    v
}
fn read_cntfrq() -> u64 {
    let v: u64;
    unsafe { core::arch::asm!("mrs {}, CNTFRQ_EL0", out(reg) v) };
    v
}

/// Отправить ICMP echo request на `dst_ip`. Требует ARP-кэш записи.
pub unsafe fn send_ping(vn: &mut virtio_net::VirtioNet, dst_ip: [u8; 4]) -> bool {
    let dst_mac = match arp::lookup(dst_ip) {
        Some(m) => m,
        None => {
            println!("ping: no ARP entry for {}.{}.{}.{}", dst_ip[0], dst_ip[1], dst_ip[2], dst_ip[3]);
            return false;
        }
    };

    // ICMP
    let mut icmp_buf = [0u8; 64];
    let payload = b"Kuberve ping!";
    let icmp_len = crate::network::icmp::build_echo_request(
        &mut icmp_buf, PING_ID, PING_SEQ, payload);

    // IPv4
    let total_len = 20 + icmp_len;
    let mut packet = [0u8; 128];
    crate::network::ip::build_ipv4_header(
        &mut packet[0..20], total_len as u16, PING_SEQ, 1, OUR_IP, dst_ip);
    packet[20..20 + icmp_len].copy_from_slice(&icmp_buf[..icmp_len]);

    // Ethernet frame
    let frame_len = 14 + total_len;
    let mut frame = [0u8; 128];
    for i in 0..6 { frame[i] = dst_mac[i]; }
    for i in 0..6 { frame[6 + i] = vn.mac[i]; }
    frame[12] = 0x08;
    frame[13] = 0x00;
    frame[14..14 + total_len].copy_from_slice(&packet[..total_len]);

    // Pad до 60 байт (Ethernet min)
    let send_len = if frame_len < 60 { 60 } else { frame_len };

    SEND_TIME = read_cntpct();
    println!("ping: 10.0.2.15 -> {}.{}.{}.{} seq={}",
             dst_ip[0], dst_ip[1], dst_ip[2], dst_ip[3], PING_SEQ);
    PING_SEQ = PING_SEQ.wrapping_add(1);

    virtio_net::send_packet(vn, &frame[..send_len])
}

/// Вызывается из `poll_rx` для IPv4-пакетов с protocol=1 (ICMP).
pub unsafe fn handle_icmp(ip_payload: &[u8], from_ip: [u8; 4]) {
    if let Some(r) = crate::network::icmp::parse_echo_reply(ip_payload) {
        if r.id == PING_ID {
            let now = read_cntpct();
            let freq = read_cntfrq();
            let ticks = now.saturating_sub(SEND_TIME);
            let rtt_ms = if freq > 0 { (ticks * 1000) / freq } else { 0 };
            println!("ping reply: {}.{}.{}.{} seq={} bytes={} time={}ms",
                     from_ip[0], from_ip[1], from_ip[2], from_ip[3],
                     r.seq, r.data_len, rtt_ms);
            REPLY_RECEIVED = true;
        }
    }
}

pub fn has_reply() -> bool { unsafe { REPLY_RECEIVED } }
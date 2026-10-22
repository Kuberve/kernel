use crate::println;
use crate::network::udp::build_udp_packet;
use crate::network::ip::build_ipv4_header;

pub const DHCP_SERVER_PORT: u16 = 67;
pub const DHCP_CLIENT_PORT: u16 = 68;
const DHCP_MAGIC: u32 = 0x63825363;

// Message types
const DHCP_DISCOVER: u8 = 1;
const DHCP_OFFER:    u8 = 2;
const DHCP_REQUEST:  u8 = 3;
const DHCP_ACK:      u8 = 5;

// Options
const OPT_MSG_TYPE:   u8 = 53;
const OPT_SERVER_ID:  u8 = 54;
const OPT_REQUESTED:  u8 = 50;
const OPT_SUBNET:     u8 = 1;
const OPT_ROUTER:     u8 = 3;
const OPT_DNS:        u8 = 6;
const OPT_END:        u8 = 255;

pub static mut OUR_IP:  [u8; 4] = [10, 0, 2, 15];   // default QEMU
pub static mut GATEWAY: [u8; 4] = [10, 0, 2, 2];
pub static mut DNS:     [u8; 4] = [10, 0, 2, 3];
pub static mut NETMASK: [u8; 4] = [255, 255, 255, 0];

static mut XID: u32 = 0;
static mut OFFER_IP: [u8; 4] = [0; 4];
static mut SERVER_ID: [u8; 4] = [0; 4];
static mut GOT_OFFER: bool = false;
static mut GOT_ACK: bool = false;
static mut IFACE_MAC: [u8; 6] = [0; 6];

pub fn our_ip() -> [u8; 4] { unsafe { OUR_IP } }
pub fn gateway() -> [u8; 4] { unsafe { GATEWAY } }

unsafe fn write_u32(buf: &mut [u8], off: usize, v: u32) {
    buf[off] = (v >> 24) as u8;
    buf[off+1] = (v >> 16) as u8;
    buf[off+2] = (v >> 8) as u8;
    buf[off+3] = v as u8;
}

unsafe fn read_u32(buf: &[u8], off: usize) -> u32 {
    ((buf[off] as u32) << 24) | ((buf[off+1] as u32) << 16)
    | ((buf[off+2] as u32) << 8) | (buf[off+3] as u32)
}

/// Построить DHCP DISCOVER или REQUEST.
fn build_dhcp(msg_type: u8, requested_ip: Option<[u8;4]>, server_id: Option<[u8;4]>) -> ([u8; 300], usize) {
    let mut buf = [0u8; 300];
    let mac = unsafe { IFACE_MAC };
    let xid = unsafe { XID };

    buf[0] = 1;              // op = BOOTREQUEST
    buf[1] = 1;              // htype = Ethernet
    buf[2] = 6;              // hlen
    buf[3] = 0;              // hops
    unsafe { write_u32(&mut buf, 4, xid); }
    buf[8] = 0; buf[9] = 0;   // secs
    buf[10] = 0x80; buf[11] = 0x00;   // flags = broadcast
    // ciaddr, yiaddr, siaddr, giaddr = 0
    for i in 0..6 { buf[28 + i] = mac[i]; }   // chaddr
    // sname, file = 0 (up to 236)
    unsafe { write_u32(&mut buf, 236, DHCP_MAGIC); }

    let mut o = 240;
    buf[o] = OPT_MSG_TYPE; buf[o+1] = 1; buf[o+2] = msg_type; o += 3;

    if let Some(ip) = requested_ip {
        buf[o] = OPT_REQUESTED; buf[o+1] = 4;
        buf[o+2..o+6].copy_from_slice(&ip); o += 6;
    }
    if let Some(sid) = server_id {
        buf[o] = OPT_SERVER_ID; buf[o+1] = 4;
        buf[o+2..o+6].copy_from_slice(&sid); o += 6;
    }

    buf[o] = OPT_END; o += 1;

    // Padding до 300 байт (DHCP min 300)
    (buf, o.max(300))
}

/// Отправить DHCP через virtio-net. src_ip = 0.0.0.0, dst = broadcast.
unsafe fn send_dhcp(vn: &mut crate::device::virtio_net::VirtioNet, buf: &[u8], len: usize) -> bool {
    use crate::device::virtio_net;

    // UDP: src=68, dst=67
    let mut udp_buf = [0u8; 320];
    let udp_len = build_udp_packet(
        &mut udp_buf,
        [0, 0, 0, 0], [255, 255, 255, 255],
        DHCP_CLIENT_PORT, DHCP_SERVER_PORT,
        &buf[..len],
    );

    // IPv4: src=0.0.0.0, dst=255.255.255.255
    let total_len = 20 + udp_len;
    let mut packet = [0u8; 340];
    build_ipv4_header(
        &mut packet[0..20], total_len as u16, 0, 17,
        [0, 0, 0, 0], [255, 255, 255, 255],
    );
    packet[20..20 + udp_len].copy_from_slice(&udp_buf[..udp_len]);

    // Ethernet: broadcast, src=our_mac
    let frame_len = 14 + total_len;
    let mut frame = [0u8; 360];
    for i in 0..6 { frame[i] = 0xFF; }
    for i in 0..6 { frame[6 + i] = vn.mac[i]; }
    frame[12] = 0x08; frame[13] = 0x00;
    frame[14..14 + total_len].copy_from_slice(&packet[..total_len]);

    let send_len = if frame_len < 60 { 60 } else { frame_len };
    virtio_net::send_packet(vn, &frame[..send_len])
}

/// Вызывается из `virtio_net::poll_rx` для UDP на порт 68.
pub unsafe fn handle_packet(udp_payload: &[u8], _src_ip: [u8; 4]) {
    if udp_payload.len() < 240 { return; }
    if read_u32(udp_payload, 236) != DHCP_MAGIC { return; }

    let my_xid = read_u32(udp_payload, 4);
    if my_xid != XID { return; }   // не наш

    // yiaddr
    let yiaddr = [
        udp_payload[16], udp_payload[17], udp_payload[18], udp_payload[19],
    ];

    // Parse options
    let mut o = 240;
    let mut msg_type = 0u8;
    let mut server_id = [0u8; 4];
    let mut subnet = [0u8; 4];
    let mut router = [0u8; 4];
    let mut dns = [0u8; 4];
    let mut got_router = false;
    let mut got_subnet = false;
    let mut got_dns = false;

    while o < udp_payload.len() {
        let opt = udp_payload[o];
        if opt == OPT_END { break; }
        if opt == 0 { o += 1; continue; }
        if o + 2 > udp_payload.len() { break; }
        let oplen = udp_payload[o + 1] as usize;
        if o + 2 + oplen > udp_payload.len() { break; }
        let val = &udp_payload[o + 2..o + 2 + oplen];

        match opt {
            OPT_MSG_TYPE if oplen == 1 => msg_type = val[0],
            OPT_SERVER_ID if oplen == 4 => server_id.copy_from_slice(val),
            OPT_SUBNET if oplen == 4 => { subnet.copy_from_slice(val); got_subnet = true; }
            OPT_ROUTER if oplen >= 4 => { router.copy_from_slice(&val[..4]); got_router = true; }
            OPT_DNS if oplen >= 4 => { dns.copy_from_slice(&val[..4]); got_dns = true; }
            _ => {}
        }
        o += 2 + oplen;
    }

    match msg_type {
        DHCP_OFFER => {
            OFFER_IP = yiaddr;
            SERVER_ID = server_id;
            GOT_OFFER = true;
            println!("DHCP: OFFER {} (server {}.{}.{}.{})",
                fmt_ip(&yiaddr),
                server_id[0], server_id[1], server_id[2], server_id[3]);
        }
        DHCP_ACK => {
            OUR_IP = yiaddr;
            if got_subnet { NETMASK = subnet; }
            if got_router { GATEWAY = router; }
            if got_dns { DNS = dns; }
            GOT_ACK = true;
            println!("DHCP: ACK {} gateway={} dns={} mask={}",
                fmt_ip(&yiaddr),
                fmt_ip(&GATEWAY), fmt_ip(&DNS), fmt_ip(&NETMASK));
        }
        _ => {}
    }
}

fn fmt_ip(ip: &[u8; 4]) -> FmtIp { FmtIp(*ip) }

struct FmtIp([u8; 4]);
impl core::fmt::Display for FmtIp {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{}.{}.{}.{}", self.0[0], self.0[1], self.0[2], self.0[3])
    }
}

/// Запустить DHCP: DISCOVER → OFFER → REQUEST → ACK.
pub unsafe fn run(vn: &mut crate::device::virtio_net::VirtioNet) -> bool {
    use crate::arch::aarch64::sched;

    IFACE_MAC = vn.mac;

    // xid = из CNTPCT
    let t: u64;
    core::arch::asm!("mrs {}, CNTPCT_EL0", out(reg) t);
    XID = (t & 0xFFFFFFFF) as u32;
    GOT_OFFER = false;
    GOT_ACK = false;

    println!("DHCP: sending DISCOVER (xid={:#x})", XID);
    let (discover, discover_len) = build_dhcp(DHCP_DISCOVER, None, None);
    if !send_dhcp(vn, &discover, discover_len) {
        println!("DHCP: DISCOVER send failed");
        return false;
    }

    // Ждём OFFER
    for _ in 0..5 {
        sched::sleep(1);
        for _ in 0..1000 {
            crate::device::virtio_net::poll_rx(vn);
        }
        if GOT_OFFER { break; }
    }
    if !GOT_OFFER {
        println!("DHCP: no OFFER, giving up");
        return false;
    }

    println!("DHCP: sending REQUEST");
    let (req, req_len) = build_dhcp(DHCP_REQUEST, Some(OFFER_IP), Some(SERVER_ID));
    if !send_dhcp(vn, &req, req_len) {
        println!("DHCP: REQUEST send failed");
        return false;
    }

    for _ in 0..5 {
        sched::sleep(1);
        for _ in 0..1000 {
            crate::device::virtio_net::poll_rx(vn);
        }
        if GOT_ACK { break; }
    }
    if !GOT_ACK {
        println!("DHCP: no ACK, giving up");
        return false;
    }

    println!("DHCP: configured IP={}", fmt_ip(&OUR_IP));
    true
}
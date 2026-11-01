use crate::println;

pub const TCP_HEADER_LEN: usize = 20;

// TCP flags (bits 9..15 в dword 12..14)
pub const FLAG_FIN: u8 = 0x01;
pub const FLAG_SYN: u8 = 0x02;
pub const FLAG_RST: u8 = 0x04;
pub const FLAG_PSH: u8 = 0x08;
pub const FLAG_ACK: u8 = 0x10;
pub const FLAG_URG: u8 = 0x20;

pub struct TcpHeader {
    pub src_port: u16,
    pub dst_port: u16,
    pub seq: u32,
    pub ack: u32,
    pub data_offset: u8,    // * 4 = header length в байтах
    pub flags: u8,
    pub window: u16,
    pub checksum: u16,
    pub urgent: u16,
    pub payload_len: usize,
}

/// Построить TCP пакет (header + payload). Возвращает полную длину.
pub fn build_tcp_packet(
    out: &mut [u8],
    src_ip: [u8; 4],
    dst_ip: [u8; 4],
    src_port: u16,
    dst_port: u16,
    seq: u32,
    ack: u32,
    flags: u8,
    window: u16,
    payload: &[u8],
) -> usize {
    let total = TCP_HEADER_LEN + payload.len();

    out[0] = (src_port >> 8) as u8;
    out[1] = src_port as u8;
    out[2] = (dst_port >> 8) as u8;
    out[3] = dst_port as u8;
    out[4] = (seq >> 24) as u8;
    out[5] = (seq >> 16) as u8;
    out[6] = (seq >> 8) as u8;
    out[7] = seq as u8;
    out[8] = (ack >> 24) as u8;
    out[9] = (ack >> 16) as u8;
    out[10] = (ack >> 8) as u8;
    out[11] = ack as u8;
    out[12] = (5u8) << 4;   // data_offset = 5 (20 байт), reserved = 0
    out[13] = flags;
    out[14] = (window >> 8) as u8;
    out[15] = window as u8;
    out[16] = 0;   // checksum placeholder
    out[17] = 0;
    out[18] = 0;   // urgent pointer
    out[19] = 0;

    out[20..20 + payload.len()].copy_from_slice(payload);

    // Pseudo-header: src IP, dst IP, zero, protocol=6, tcp_len
    let mut pseudo = [0u8; 12];
    pseudo[0..4].copy_from_slice(&src_ip);
    pseudo[4..8].copy_from_slice(&dst_ip);
    pseudo[8] = 0;
    pseudo[9] = 6;   // TCP
    pseudo[10] = ((total as u16) >> 8) as u8;
    pseudo[11] = (total as u16) as u8;

    // Checksum over pseudo + tcp
    let mut sum: u32 = 0;
    for chunk in pseudo.chunks(2) {
        sum += ((chunk[0] as u32) << 8) | (chunk.get(1).copied().unwrap_or(0) as u32);
    }
    let tcp_data = &out[..total];
    let mut i = 0;
    while i + 1 < tcp_data.len() {
        sum += ((tcp_data[i] as u32) << 8) | (tcp_data[i + 1] as u32);
        i += 2;
    }
    if i < tcp_data.len() {
        sum += (tcp_data[i] as u32) << 8;
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    let cs = !sum as u16;
    out[16] = (cs >> 8) as u8;
    out[17] = cs as u8;

    total
}

pub fn parse_tcp_header(tcp: &[u8]) -> Option<TcpHeader> {
    if tcp.len() < TCP_HEADER_LEN { return None; }
    let src_port = ((tcp[0] as u16) << 8) | tcp[1] as u16;
    let dst_port = ((tcp[2] as u16) << 8) | tcp[3] as u16;
    let seq = ((tcp[4] as u32) << 24) | ((tcp[5] as u32) << 16)
            | ((tcp[6] as u32) << 8) | (tcp[7] as u32);
    let ack = ((tcp[8] as u32) << 24) | ((tcp[9] as u32) << 16)
            | ((tcp[10] as u32) << 8) | (tcp[11] as u32);
    let data_offset = tcp[12] >> 4;
    let flags = tcp[13];
    let window = ((tcp[14] as u16) << 8) | tcp[15] as u16;
    let checksum = ((tcp[16] as u16) << 8) | tcp[17] as u16;
    let urgent = ((tcp[18] as u16) << 8) | tcp[19] as u16;

    let hdr_len = (data_offset as usize) * 4;
    if hdr_len < TCP_HEADER_LEN || hdr_len > tcp.len() { return None; }
    let payload_len = tcp.len() - hdr_len;

    Some(TcpHeader {
        src_port, dst_port, seq, ack,
        data_offset, flags, window, checksum, urgent,
        payload_len,
    })
}

/// Декодировать flags в человекочитаемую строку
pub fn flags_str(flags: u8) -> &'static str {
    match flags {
        0x02 => "SYN",
        0x12 => "SYN-ACK",
        0x10 => "ACK",
        0x18 => "PSH-ACK",
        0x11 => "FIN-ACK",
        0x01 => "FIN",
        0x04 => "RST",
        0x14 => "RST-ACK",
        _ => "?",
    }
}

// ============================================================
// TCP connection state
// ============================================================


#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TcpState {
    Closed,
    Listen,
    SynSent,
    SynReceived,
    Established,
    FinWait1,
    FinWait2,
    CloseWait,
    Closing,
    LastAck,
    TimeWait,
}

pub const MAX_TCP_CONNS: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TcpRole {
    None,
    Server,
    Client,
}

const TCP_RX_BUF: usize = 1024;

#[derive(Clone, Copy)]
pub struct TcpConn {
    pub state: TcpState,
    pub local_port: u16,
    pub remote_ip: [u8; 4],
    pub remote_port: u16,
    pub local_seq: u32,
    pub remote_seq: u32,
    pub local_ack: u32,
    pub window: u16,
    pub used: bool,
    pub fd: i64,
    pub role: TcpRole,
    pub rx_buf: [u8; TCP_RX_BUF],
    pub rx_len: u16,
    pub saw_syn_ack: bool,
}

pub const EMPTY_TCP_CONN: TcpConn = TcpConn {
    state: TcpState::Closed,
    local_port: 0,
    remote_ip: [0; 4],
    remote_port: 0,
    local_seq: 0,
    remote_seq: 0,
    local_ack: 0,
    window: 0,
    used: false,
    fd: -1,
    role: TcpRole::None,
    rx_buf: [0; TCP_RX_BUF],
    rx_len: 0,
    saw_syn_ack: false,
};

static mut CONNS: [TcpConn; MAX_TCP_CONNS] = [EMPTY_TCP_CONN; MAX_TCP_CONNS];

pub fn init() {
    unsafe {
        for i in 0..MAX_TCP_CONNS {
            CONNS[i] = EMPTY_TCP_CONN;
        }
    }
}

/// Найти соединение по 4-tuple.
pub unsafe fn find(src_ip: [u8; 4], src_port: u16, dst_port: u16) -> Option<usize> {
    for i in 0..MAX_TCP_CONNS {
        if CONNS[i].used
            && CONNS[i].remote_ip == src_ip
            && CONNS[i].remote_port == src_port
            && CONNS[i].local_port == dst_port
        {
            return Some(i);
        }
    }
    None
}

/// Найти free slot.
pub unsafe fn alloc_slot() -> Option<usize> {
    for i in 0..MAX_TCP_CONNS {
        if !CONNS[i].used {
            return Some(i);
        }
    }
    None
}

pub unsafe fn get_mut(id: usize) -> Option<&'static mut TcpConn> {
    if id >= MAX_TCP_CONNS { return None; }
    Some(&mut CONNS[id])
}

pub unsafe fn get(id: usize) -> Option<&'static TcpConn> {
    if id >= MAX_TCP_CONNS { return None; }
    Some(&CONNS[id])
}

/// Отправить TCP сегмент. Собирает IPv4 + TCP + Ethernet, шлёт через virtio-net.
pub unsafe fn send_tcp_segment(
    dst_ip: [u8; 4],
    dst_port: u16,
    src_port: u16,
    seq: u32,
    ack: u32,
    flags: u8,
    window: u16,
    payload: &[u8],
) -> bool {
    use crate::drivers::virtio::net as virtio_net;
use crate::network::arp;

    let vn = match virtio_net::get_global_mut() {
        Some(v) => v,
        None => return false,
    };

    let dst_mac = match arp::lookup(dst_ip) {
        Some(m) => m,
        None => return false,
    };

    let our_ip = crate::network::dhcp::our_ip();

    // TCP segment
    let mut tcp_buf = [0u8; 1400];
    let tcp_len = build_tcp_packet(
        &mut tcp_buf, our_ip, dst_ip, src_port, dst_port,
        seq, ack, flags, window, payload);

    // IPv4
    let total_len = 20 + tcp_len;
    let mut packet = [0u8; 1420];
    crate::network::ip::build_ipv4_header(
        &mut packet[0..20], total_len as u16, 0, 6, our_ip, dst_ip);
    packet[20..20 + tcp_len].copy_from_slice(&tcp_buf[..tcp_len]);

    // Ethernet
    let frame_len = 14 + total_len;
    let mut frame = [0u8; 1440];
    for i in 0..6 { frame[i] = dst_mac[i]; }
    for i in 0..6 { frame[6 + i] = vn.mac[i]; }
    frame[12] = 0x08;
    frame[13] = 0x00;
    frame[14..14 + total_len].copy_from_slice(&packet[..total_len]);

    let send_len = if frame_len < 60 { 60 } else { frame_len };

    virtio_net::send_packet(vn, &frame[..send_len])
}

// Слушающие порты (пока один — 8080)
static mut LISTEN_PORT: u16 = 0;

pub unsafe fn listen(port: u16) {
    LISTEN_PORT = port;
    println!("TCP: listening on :{}", port);
}

fn gen_isn() -> u32 {
    let v: u64;
    unsafe { core::arch::asm!("mrs {}, CNTPCT_EL0", out(reg) v) };
    (v & 0xFFFFFFFF) as u32
}

pub unsafe fn handle_incoming(
    src_ip: [u8; 4],
    h: &TcpHeader,
    _payload: &[u8],
) {
    // 1. Есть ли существующее соединение?
    let conn_id = find(src_ip, h.src_port, h.dst_port);

    if let Some(id) = conn_id {
        let c = get_mut(id).unwrap();
        match c.state {
            TcpState::FinWait1 => {
                if h.flags & FLAG_ACK != 0 && h.ack == c.local_seq {
                    if h.flags & FLAG_FIN != 0 {
                        c.remote_seq = c.remote_seq.wrapping_add(1);
                        send_tcp_segment(src_ip, h.src_port, h.dst_port,
                            c.local_seq, c.remote_seq, FLAG_ACK, 65535, &[]);
                        c.state = TcpState::Closed;
                        c.used = false;
                        println!("TCP client: closed (FinWait1 → FIN-ACK)");
                    } else {
                        c.state = TcpState::FinWait2;
                        println!("TCP client: FinWait2 (нашего FIN подтвердили)");
                    }
                }
            }
            TcpState::FinWait2 => {
                if h.flags & FLAG_FIN != 0 {
                    c.remote_seq = c.remote_seq.wrapping_add(1);
                    send_tcp_segment(src_ip, h.src_port, h.dst_port,
                        c.local_seq, c.remote_seq, FLAG_ACK, 65535, &[]);
                    c.state = TcpState::Closed;
                    c.used = false;
                    println!("TCP client: closed (FinWait2 → FIN)");
                }
            }
            TcpState::SynSent => {
                if h.flags & FLAG_SYN != 0 && h.flags & FLAG_ACK != 0
                   && h.ack == c.local_seq.wrapping_add(1) {
                    c.remote_seq = h.seq.wrapping_add(1);
                    c.local_seq = c.local_seq.wrapping_add(1);
                    c.state = TcpState::Established;
                    c.saw_syn_ack = true;
                    println!("TCP client: ESTABLISHED to {}.{}.{}.{}:{}",
                             src_ip[0], src_ip[1], src_ip[2], src_ip[3], h.src_port);
                    send_tcp_segment(src_ip, h.src_port, h.dst_port,
                        c.local_seq, c.remote_seq, FLAG_ACK, 65535, &[]);
                }
            }
            TcpState::SynReceived => {
                // Ждём ACK: клиент должен подтвердить наш SYN (our_isn+1)
                if h.flags & FLAG_ACK != 0 && h.ack == c.local_seq.wrapping_add(1) {
                    c.state = TcpState::Established;
                    c.local_seq = c.local_seq.wrapping_add(1);   // snd_nxt: SYN съел 1
                    // Обновить remote_seq: клиентский seq + payload
                    c.remote_seq = h.seq.wrapping_add(h.payload_len as u32);
                    println!("TCP: ESTABLISHED {}.{}.{}.{}:{} <-> :{}",
                             src_ip[0], src_ip[1], src_ip[2], src_ip[3],
                             c.remote_port, c.local_port);
                }
            }
            TcpState::Established => {
                if h.flags & FLAG_ACK != 0 {
                    c.local_ack = h.ack;
                }

                if _payload.len() > 0 {
                    if h.seq == c.remote_seq {
                        c.remote_seq = c.remote_seq.wrapping_add(_payload.len() as u32);
                        println!("TCP: recv {} bytes", _payload.len());
                        send_tcp_segment(src_ip, h.src_port, h.dst_port,
                            c.local_seq, c.remote_seq, FLAG_ACK, 65535, &[]);

                        if c.role == TcpRole::Server {
                            // Echo
                            send_tcp_segment(src_ip, h.src_port, h.dst_port,
                                c.local_seq, c.remote_seq, FLAG_PSH | FLAG_ACK, 65535, _payload);
                            c.local_seq = c.local_seq.wrapping_add(_payload.len() as u32);
                        } else {
                            // Client: буферизуем
                            let space = TCP_RX_BUF - c.rx_len as usize;
                            let n = core::cmp::min(space, _payload.len());
                            for i in 0..n {
                                c.rx_buf[c.rx_len as usize + i] = _payload[i];
                            }
                            c.rx_len += n as u16;
                            println!("TCP client: buffered {} bytes (rx_len={})", n, c.rx_len);
                        }
                    } else {
                        println!("TCP: out-of-order seq={} expect={}", h.seq, c.remote_seq);
                        send_tcp_segment(src_ip, h.src_port, h.dst_port,
                            c.local_seq, c.remote_seq, FLAG_ACK, 65535, &[]);
                    }
                }

                if h.flags & FLAG_FIN != 0 {
                    c.remote_seq = c.remote_seq.wrapping_add(1);
                    if c.role == TcpRole::Server {
                        c.state = TcpState::CloseWait;
                        send_tcp_segment(src_ip, h.src_port, h.dst_port,
                            c.local_seq, c.remote_seq, FLAG_FIN | FLAG_ACK, 65535, &[]);
                        c.local_seq = c.local_seq.wrapping_add(1);
                        c.state = TcpState::LastAck;
                    } else {
                        c.state = TcpState::Closed;
                        println!("TCP client: server FIN, closed");
                    }
                }

                if c.state == TcpState::LastAck && h.flags & FLAG_ACK != 0 && h.ack == c.local_seq {
                    c.state = TcpState::Closed;
                    c.used = false;
                }
            }
            _ => {}
        }
        return;
    }

    // 2. Нет соединения — это SYN на listen_port?
    if h.flags & FLAG_SYN != 0 && (h.flags & FLAG_ACK) == 0 && h.dst_port == LISTEN_PORT {
        let slot = match alloc_slot() {
            Some(s) => s,
            None => {
                println!("TCP: no free conn slot, dropping SYN");
                return;
            }
        };
        let our_isn = gen_isn();

        let c = get_mut(slot).unwrap();
        c.used = true;
        c.state = TcpState::SynReceived;
        c.local_port = h.dst_port;
        c.remote_ip = src_ip;
        c.remote_port = h.src_port;
        c.local_seq = our_isn;
        c.remote_seq = h.seq.wrapping_add(1);   // SYN съедает 1
        c.local_ack = h.seq.wrapping_add(1);
        c.window = 65535;

        println!("TCP: recv SYN {}.{}.{}.{}:{} -> :{}, sending SYN-ACK (our_isn={:#x})",
                 src_ip[0], src_ip[1], src_ip[2], src_ip[3], h.src_port, h.dst_port, our_isn);

        // Отправить SYN-ACK: seq=our_isn, ack=client_isn+1
        send_tcp_segment(
            src_ip, h.src_port, h.dst_port,
            our_isn, h.seq.wrapping_add(1),
            FLAG_SYN | FLAG_ACK,
            65535, &[],
        );
    }
}

/// Отправить данные через установленное соединение.
pub unsafe fn send_data(conn_id: usize, data: &[u8]) -> bool {
    let c = match get(conn_id) {
        Some(c) => c,
        None => return false,
    };
    if c.state != TcpState::Established { return false; }
    let src_ip = c.remote_ip;
    let dst_port = c.remote_port;
    let src_port = c.local_port;
    let seq = c.local_seq;
    let ack = c.remote_seq;

    let ok = send_tcp_segment(
        src_ip, dst_port, src_port,
        seq, ack,
        FLAG_PSH | FLAG_ACK, 65535, data,
    );
    if ok {
        if let Some(c2) = get_mut(conn_id) {
            c2.local_seq = c2.local_seq.wrapping_add(data.len() as u32);
        }
    }
    ok
}

pub unsafe fn active_connection() -> Option<usize> {
    for i in 0..MAX_TCP_CONNS {
        if CONNS[i].used && CONNS[i].state == TcpState::Established {
            return Some(i);
        }
    }
    None
}

pub unsafe fn find_by_fd(fd: i64) -> Option<usize> {
    for i in 0..MAX_TCP_CONNS {
        if CONNS[i].used && CONNS[i].fd == fd {
            return Some(i);
        }
    }
    None
}

pub unsafe fn connect(fd: i64, dst_ip: [u8; 4], dst_port: u16) -> i64 {
    use crate::arch::aarch64::sched;

    let slot = match alloc_slot() {
        Some(s) => s,
        None => return -1,
    };
    let our_isn = gen_isn();
    let our_port = 49152u16.wrapping_add((slot as u16) * 17);

    {
        let c = get_mut(slot).unwrap();
        *c = EMPTY_TCP_CONN;
        c.used = true;
        c.state = TcpState::SynSent;
        c.local_port = our_port;
        c.remote_ip = dst_ip;
        c.remote_port = dst_port;
        c.local_seq = our_isn;
        c.window = 65535;
        c.fd = fd;
        c.role = TcpRole::Client;
    }

    println!("TCP client: connect {}.{}.{}.{}:{} (isn={:#x}, port={})",
             dst_ip[0], dst_ip[1], dst_ip[2], dst_ip[3], dst_port, our_isn, our_port);

    if !send_tcp_segment(dst_ip, dst_port, our_port, our_isn, 0, FLAG_SYN, 65535, &[]) {
        println!("TCP client: SYN send FAILED");
        get_mut(slot).unwrap().used = false;
        return -1;
    }

    for _ in 0..10 {
        sched::sleep(1);
        let c = get(slot).unwrap();
        if c.saw_syn_ack {
            return slot as i64;  // успех: SYN-ACK получен
        }
    }
    println!("TCP client: connect timeout");
    get_mut(slot).unwrap().used = false;
    -1
}

pub unsafe fn tcp_send(fd: i64, data: &[u8]) -> i64 {
    let slot = match find_by_fd(fd) {
        Some(s) => s,
        None => return -1,
    };
    let c = get(slot).unwrap();
    if c.state != TcpState::Established { return -1; }
    if !send_data(slot, data) { return -1; }
    data.len() as i64
}

pub unsafe fn tcp_recv(fd: i64, out: &mut [u8]) -> i64 {
    let slot = match find_by_fd(fd) {
        Some(s) => s,
        None => return -1,
    };
    let c = get_mut(slot).unwrap();
    if c.rx_len == 0 { return 0; }
    let n = core::cmp::min(out.len(), c.rx_len as usize);
    out[..n].copy_from_slice(&c.rx_buf[..n]);
    let rem = c.rx_len as usize - n;
    for i in 0..rem {
        c.rx_buf[i] = c.rx_buf[n + i];
    }
    c.rx_len = rem as u16;
    n as i64
}

pub unsafe fn close(fd: i64) -> i64 {
    let slot = match find_by_fd(fd) {
        Some(s) => s,
        None => return -1,
    };
    let c = get_mut(slot).unwrap();

    if c.state != TcpState::Established && c.state != TcpState::CloseWait {
        c.used = false;
        return 0;
    }

    let ok = send_tcp_segment(
        c.remote_ip, c.remote_port, c.local_port,
        c.local_seq, c.remote_seq,
        FLAG_FIN | FLAG_ACK, 65535, &[],
    );
    if !ok { return -1; }
    c.local_seq = c.local_seq.wrapping_add(1);
    c.state = TcpState::FinWait1;
    println!("TCP client: FIN sent, FinWait1");
    0
}
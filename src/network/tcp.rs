use crate::network::ip::checksum;

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
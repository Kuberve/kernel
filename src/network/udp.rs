pub const UDP_HEADER_LEN: usize = 8;

/// Построить UDP пакет. Возвращает полную длину (header + payload).
pub fn build_udp_packet(
    out: &mut [u8],
    src_ip: [u8; 4],
    dst_ip: [u8; 4],
    src_port: u16,
    dst_port: u16,
    payload: &[u8],
) -> usize {
    let udp_len = UDP_HEADER_LEN + payload.len();
    out[0] = (src_port >> 8) as u8;
    out[1] = src_port as u8;
    out[2] = (dst_port >> 8) as u8;
    out[3] = dst_port as u8;
    out[4] = (udp_len >> 8) as u8;
    out[5] = udp_len as u8;
    out[6] = 0;   // checksum placeholder
    out[7] = 0;

    out[8..8 + payload.len()].copy_from_slice(payload);

    // Pseudo-header для checksum: src IP, dst IP, zero, protocol=17, udp_len
    let mut pseudo = [0u8; 12];
    pseudo[0..4].copy_from_slice(&src_ip);
    pseudo[4..8].copy_from_slice(&dst_ip);
    pseudo[8] = 0;
    pseudo[9] = 17;   // UDP
    pseudo[10] = (udp_len >> 8) as u8;
    pseudo[11] = udp_len as u8;

    // Checksum over pseudo + udp
    let mut sum: u32 = 0;
    for chunk in pseudo.chunks(2) {
        sum += ((chunk[0] as u32) << 8) | (chunk.get(1).copied().unwrap_or(0) as u32);
    }
    let udp_data = &out[..udp_len];
    let mut i = 0;
    while i + 1 < udp_data.len() {
        sum += ((udp_data[i] as u32) << 8) | (udp_data[i + 1] as u32);
        i += 2;
    }
    if i < udp_data.len() {
        sum += (udp_data[i] as u32) << 8;
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    let cs = !sum as u16;
    // По RFC 768: если checksum = 0, пишем 0xFFFF
    let cs = if cs == 0 { 0xFFFF } else { cs };
    out[6] = (cs >> 8) as u8;
    out[7] = cs as u8;

    udp_len
}

pub struct UdpHeader {
    pub src_port: u16,
    pub dst_port: u16,
    pub length: u16,
}

pub fn parse_udp_header(udp: &[u8]) -> Option<UdpHeader> {
    if udp.len() < UDP_HEADER_LEN { return None; }
    let src_port = ((udp[0] as u16) << 8) | udp[1] as u16;
    let dst_port = ((udp[2] as u16) << 8) | udp[3] as u16;
    let length = ((udp[4] as u16) << 8) | udp[5] as u16;
    Some(UdpHeader { src_port, dst_port, length })
}
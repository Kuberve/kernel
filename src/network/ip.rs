/// 16-bit one's complement checksum (RFC 1071).
/// Возвращает значение, готовое к записи в header (уже complemented).
pub fn checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 1 < data.len() {
        sum += ((data[i] as u32) << 8) | (data[i + 1] as u32);
        i += 2;
    }
    if i < data.len() {
        sum += (data[i] as u32) << 8;
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !sum as u16
}

/// Записать IPv4 header (20 байт) в out[0..20].
/// protocol: 1=ICMP, 6=TCP, 17=UDP
pub fn build_ipv4_header(
    out: &mut [u8],
    total_len: u16,
    id: u16,
    protocol: u8,
    src: [u8; 4],
    dst: [u8; 4],
) {
    out[0] = 0x45;               // version=4, IHL=5
    out[1] = 0;                  // DSCP/ECN
    out[2] = (total_len >> 8) as u8;
    out[3] = total_len as u8;
    out[4] = (id >> 8) as u8;
    out[5] = id as u8;
    out[6] = 0x40;               // DF (don't fragment), offset=0
    out[7] = 0;
    out[8] = 64;                 // TTL
    out[9] = protocol;
    out[10] = 0;                 // checksum placeholder
    out[11] = 0;
    out[12..16].copy_from_slice(&src);
    out[16..20].copy_from_slice(&dst);

    let cs = checksum(&out[0..20]);
    out[10] = (cs >> 8) as u8;
    out[11] = cs as u8;
}
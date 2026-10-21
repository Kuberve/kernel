use crate::network::ip::checksum;

pub const ICMP_ECHO_REQUEST: u8 = 8;
pub const ICMP_ECHO_REPLY: u8 = 0;

/// Построить ICMP echo request. Возвращает полную длину (header + payload).
pub fn build_echo_request(out: &mut [u8], id: u16, seq: u16, payload: &[u8]) -> usize {
    out[0] = ICMP_ECHO_REQUEST;
    out[1] = 0;   // code
    out[2] = 0;   // checksum placeholder
    out[3] = 0;
    out[4] = (id >> 8) as u8;
    out[5] = id as u8;
    out[6] = (seq >> 8) as u8;
    out[7] = seq as u8;

    let copy_len = core::cmp::min(payload.len(), out.len() - 8);
    out[8..8 + copy_len].copy_from_slice(&payload[..copy_len]);

    let total = 8 + copy_len;
    let cs = checksum(&out[..total]);
    out[2] = (cs >> 8) as u8;
    out[3] = cs as u8;

    total
}

pub struct EchoReply {
    pub id: u16,
    pub seq: u16,
    pub data_len: usize,
}

/// Проверить, что это ICMP echo reply, и распарсить.
pub fn parse_echo_reply(icmp: &[u8]) -> Option<EchoReply> {
    if icmp.len() < 8 { return None; }
    if icmp[0] != ICMP_ECHO_REPLY { return None; }
    let id = ((icmp[4] as u16) << 8) | icmp[5] as u16;
    let seq = ((icmp[6] as u16) << 8) | icmp[7] as u16;
    Some(EchoReply { id, seq, data_len: icmp.len() - 8 })
}
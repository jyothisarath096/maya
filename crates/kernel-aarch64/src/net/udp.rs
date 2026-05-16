pub struct UdpPacket {
    buf: [u8; 1500],
    len: usize,
}

impl UdpPacket {
    pub fn new(
        src_mac: [u8; 6],
        dst_mac: [u8; 6],
        src_ip: [u8; 4],
        dst_ip: [u8; 4],
        src_port: u16,
        dst_port: u16,
        payload: &[u8],
    ) -> Self {
        let mut buf = [0u8; 1500];
        let plen = payload.len().min(1400);

        buf[0..6].copy_from_slice(&dst_mac);
        buf[6..12].copy_from_slice(&src_mac);
        buf[12] = 0x08;
        buf[13] = 0x00;

        buf[14] = 0x45;
        buf[15] = 0x00;
        let total = 20 + 8 + plen;
        buf[16] = (total >> 8) as u8;
        buf[17] = total as u8;
        buf[18] = 0;
        buf[19] = 0;
        buf[20] = 0x40;
        buf[21] = 0x00;
        buf[22] = 64;
        buf[23] = 0x11;
        buf[24] = 0;
        buf[25] = 0;
        buf[26..30].copy_from_slice(&src_ip);
        buf[30..34].copy_from_slice(&dst_ip);
        let csum = ip_checksum(&buf[14..34]);
        buf[24] = (csum >> 8) as u8;
        buf[25] = csum as u8;

        buf[34] = (src_port >> 8) as u8;
        buf[35] = src_port as u8;
        buf[36] = (dst_port >> 8) as u8;
        buf[37] = dst_port as u8;
        let udp_len = 8 + plen;
        buf[38] = (udp_len >> 8) as u8;
        buf[39] = udp_len as u8;
        buf[40] = 0;
        buf[41] = 0;

        buf[42..42 + plen].copy_from_slice(&payload[..plen]);
        Self { buf, len: 42 + plen }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}

pub fn ip_checksum(header: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    for i in (0..header.len()).step_by(2) {
        let hi = header[i] as u32;
        let lo = if i + 1 < header.len() { header[i + 1] as u32 } else { 0 };
        sum = sum.wrapping_add((hi << 8) | lo);
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}


//! mgmt_util.rs — shared formatting helpers for mgmt console

pub fn fmt_hex8(s: &mut heapless::String<80>, b: u8) {
    const H: &[u8] = b"0123456789ABCDEF";
    let _ = s.push(H[(b >> 4) as usize] as char);
    let _ = s.push(H[(b & 0xF) as usize] as char);
}

pub fn fmt_hex32(s: &mut heapless::String<80>, n: u32) {
    const H: &[u8] = b"0123456789ABCDEF";
    for shift in [28u32, 24, 20, 16, 12, 8, 4, 0] {
        let _ = s.push(H[((n >> shift) & 0xF) as usize] as char);
    }
}

pub fn push_num(s: &mut heapless::String<32>, n: usize) {
    if n == 0 { let _ = s.push('0'); return; }
    let mut buf = [0u8; 10];
    let mut i = 10usize;
    let mut v = n;
    while v > 0 { i -= 1; buf[i] = b'0' + (v % 10) as u8; v /= 10; }
    for &b in &buf[i..] { let _ = s.push(b as char); }
}

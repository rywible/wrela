pub fn checksum(data: &[u8]) -> u64 {
    let mut acc = 1469598103934665603u64;
    for b in data {
        acc ^= *b as u64;
        acc = acc.wrapping_mul(1099511628211u64);
    }
    acc
}

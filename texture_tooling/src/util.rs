pub fn deterministic_hash(parts: &[&[u8]]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for part in parts {
        for byte in *part {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

pub fn expected_byte_len(width: u32, height: u32, bytes_per_pixel: u32) -> Result<usize, String> {
    let pixel_count = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or_else(|| "texture dimensions overflow pixel count".to_string())?;

    let byte_count = pixel_count
        .checked_mul(u64::from(bytes_per_pixel))
        .ok_or_else(|| "texture dimensions overflow byte count".to_string())?;

    usize::try_from(byte_count).map_err(|_| "texture byte count exceeds platform usize".to_string())
}

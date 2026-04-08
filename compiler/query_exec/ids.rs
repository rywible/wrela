use smol_str::SmolStr;

pub fn stable_portable_id(parts: &[&[u8]]) -> u32 {
    const FNV_OFFSET_BASIS: u32 = 0x811c9dc5;
    const FNV_PRIME: u32 = 0x0100_0193;
    let mut hash = FNV_OFFSET_BASIS;
    for part in parts {
        for byte in *part {
            hash ^= *byte as u32;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    hash
}

pub fn stable_shape_capture_id(shape_name: &SmolStr) -> u32 {
    stable_portable_id(&[shape_name.as_bytes()])
}

pub fn stable_shape_scene_capture_id(shape_name: &SmolStr) -> u32 {
    stable_portable_id(&[b"scene::shape::", shape_name.as_bytes()])
}

pub fn stable_field_scene_capture_id(field_name: &SmolStr) -> u32 {
    stable_portable_id(&[b"scene::field::", field_name.as_bytes()])
}

pub fn stable_region_scene_capture_id(region_name: &SmolStr) -> u32 {
    stable_portable_id(&[b"scene::region::", region_name.as_bytes()])
}

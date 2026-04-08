use wrela::portable::{
    PortableAbiType, PortableBuiltinType, PortableStructField, builtin_records,
    portable_abi_array_stride, portable_abi_field_offset, portable_abi_layout,
    portable_builtin_record_abi,
};

#[test]
fn portable_abi_matches_wgsl_layout_rules_for_scalars_and_matrices() {
    let bool_layout = portable_abi_layout(&PortableAbiType::Bool);
    assert_eq!(bool_layout.size, 4);
    assert_eq!(bool_layout.align, 4);

    let vec3_layout = portable_abi_layout(&PortableAbiType::Vec3);
    assert_eq!(vec3_layout.size, 12);
    assert_eq!(vec3_layout.align, 16);

    let mat3_layout = portable_abi_layout(&PortableAbiType::Mat3);
    assert_eq!(mat3_layout.size, 48);
    assert_eq!(mat3_layout.align, 16);
}

#[test]
fn portable_abi_arrays_use_wgsl_stride_for_padded_elements() {
    let vec3_array = PortableAbiType::Array(Box::new(PortableAbiType::Vec3), 3);
    let vec3_array_layout = portable_abi_layout(&vec3_array);
    assert_eq!(portable_abi_array_stride(&PortableAbiType::Vec3), 16);
    assert_eq!(vec3_array_layout.size, 48);
    assert_eq!(vec3_array_layout.align, 16);

    let padded_struct = PortableAbiType::Struct {
        name: "PaddedStruct".into(),
        class_id: 0,
        fields: vec![
            PortableStructField {
                name: "basis".into(),
                ty: PortableAbiType::Mat3,
            },
            PortableStructField {
                name: "tag".into(),
                ty: PortableAbiType::U32,
            },
        ],
    };
    let padded_struct_layout = portable_abi_layout(&padded_struct);
    assert_eq!(padded_struct_layout.size, 64);
    assert_eq!(portable_abi_array_stride(&padded_struct), 64);

    let outer = PortableAbiType::Struct {
        name: "Outer".into(),
        class_id: 0,
        fields: vec![
            PortableStructField {
                name: "items".into(),
                ty: PortableAbiType::Array(Box::new(padded_struct.clone()), 2),
            },
            PortableStructField {
                name: "trailer".into(),
                ty: PortableAbiType::U32,
            },
        ],
    };
    let PortableAbiType::Struct { fields, .. } = &outer else {
        unreachable!();
    };
    assert_eq!(portable_abi_field_offset(fields, 1), 128);
    assert_eq!(portable_abi_layout(&outer).size, 144);
}

#[test]
fn portable_builtin_records_are_32_bit_clean() {
    for record in builtin_records() {
        for field in record.fields {
            match field.ty {
                PortableBuiltinType::Atom(_) => {}
                PortableBuiltinType::Named(name) => {
                    assert!(
                        portable_builtin_record_abi(name).is_some(),
                        "builtin record field {}.{} should resolve to a portable ABI type",
                        record.name,
                        field.name
                    );
                }
            }
        }
    }
}

#[test]
fn hit3_layout_preserves_wgsl_padding_boundaries() {
    let PortableAbiType::Struct { fields, .. } = portable_builtin_record_abi("Hit3").unwrap()
    else {
        panic!("Hit3 should lower to a struct ABI");
    };
    let layout = portable_abi_layout(&portable_builtin_record_abi("Hit3").unwrap());
    assert_eq!(layout.size, 256);
    assert_eq!(layout.align, 16);

    let position = fields
        .iter()
        .position(|field| field.name.as_str() == "position")
        .unwrap();
    let normal = fields
        .iter()
        .position(|field| field.name.as_str() == "normal")
        .unwrap();
    let shading_frame = fields
        .iter()
        .position(|field| field.name.as_str() == "shading_frame")
        .unwrap();
    let payload = fields
        .iter()
        .position(|field| field.name.as_str() == "payload")
        .unwrap();

    assert_eq!(portable_abi_field_offset(&fields, position), 16);
    assert_eq!(portable_abi_field_offset(&fields, normal), 32);
    assert_eq!(portable_abi_field_offset(&fields, shading_frame), 80);
    assert_eq!(portable_abi_field_offset(&fields, payload), 228);
}

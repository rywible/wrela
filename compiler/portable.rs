pub mod abi;

pub use abi::{
    PortableAbiError, PortableAbiLayout, PortableAbiType, PortableStructField, align_to_u32,
    portable_abi_array_stride, portable_abi_decode_slice, portable_abi_decode_value,
    portable_abi_emit_wgsl_structs, portable_abi_encode_slice, portable_abi_encode_value,
    portable_abi_field_offset, portable_abi_lane_offset, portable_abi_layout,
    portable_abi_wgsl_type_name, portable_artifact_contract_abi, portable_candidate_contract_abi,
    portable_dispatch_contract_abi, portable_hit_context_contract_abi,
    portable_participant_contract_abi, portable_query_item_abi, portable_query_result_abi,
    portable_result_contract_abi,
};
use smol_str::SmolStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PortableBuiltinAtom {
    Bool,
    I32,
    U32,
    F32,
    Vec2,
    Vec3,
    Vec4,
    Mat3,
    Mat4,
    Quat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PortableBuiltinType {
    Atom(PortableBuiltinAtom),
    Named(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PortableBuiltinField {
    pub name: &'static str,
    pub ty: PortableBuiltinType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortableBuiltinRecord {
    pub name: &'static str,
    pub function_name: Option<&'static str>,
    pub constructible: bool,
    pub fields: &'static [PortableBuiltinField],
}

use PortableBuiltinAtom as Atom;
use PortableBuiltinType::{Atom as TyAtom, Named as TyNamed};

const BOUNDS2_FIELDS: &[PortableBuiltinField] = &[
    PortableBuiltinField {
        name: "min",
        ty: TyAtom(Atom::Vec2),
    },
    PortableBuiltinField {
        name: "max",
        ty: TyAtom(Atom::Vec2),
    },
];

const BOUNDS3_FIELDS: &[PortableBuiltinField] = &[
    PortableBuiltinField {
        name: "min",
        ty: TyAtom(Atom::Vec3),
    },
    PortableBuiltinField {
        name: "max",
        ty: TyAtom(Atom::Vec3),
    },
];

const RAY3_FIELDS: &[PortableBuiltinField] = &[
    PortableBuiltinField {
        name: "origin",
        ty: TyAtom(Atom::Vec3),
    },
    PortableBuiltinField {
        name: "direction",
        ty: TyAtom(Atom::Vec3),
    },
];

const DISTANCE_RESULT_FIELDS: &[PortableBuiltinField] = &[PortableBuiltinField {
    name: "distance",
    ty: TyAtom(Atom::F32),
}];

const NORMAL_RESULT_FIELDS: &[PortableBuiltinField] = &[PortableBuiltinField {
    name: "normal",
    ty: TyAtom(Atom::Vec3),
}];

const OCCLUSION_RESULT_FIELDS: &[PortableBuiltinField] = &[
    PortableBuiltinField {
        name: "occluded",
        ty: TyAtom(Atom::Bool),
    },
    PortableBuiltinField {
        name: "distance",
        ty: TyAtom(Atom::F32),
    },
    PortableBuiltinField {
        name: "steps",
        ty: TyAtom(Atom::I32),
    },
];

const TRANSFORM3_FIELDS: &[PortableBuiltinField] = &[
    PortableBuiltinField {
        name: "matrix",
        ty: TyAtom(Atom::Mat4),
    },
    PortableBuiltinField {
        name: "inverse",
        ty: TyAtom(Atom::Mat4),
    },
];

const SCENE_CAPTURE_FIELDS: &[PortableBuiltinField] = &[
    PortableBuiltinField {
        name: "scene_id",
        ty: TyAtom(Atom::U32),
    },
    PortableBuiltinField {
        name: "epoch",
        ty: TyAtom(Atom::U32),
    },
    PortableBuiltinField {
        name: "root_feature_id",
        ty: TyAtom(Atom::U32),
    },
];

const DISPATCH_BACKEND_FIELDS: &[PortableBuiltinField] = &[PortableBuiltinField {
    name: "id",
    ty: TyAtom(Atom::I32),
}];

const SPATIAL_DOMAIN_CONTRACT_FIELDS: &[PortableBuiltinField] = &[
    PortableBuiltinField {
        name: "geometry_detail",
        ty: TyAtom(Atom::I32),
    },
    PortableBuiltinField {
        name: "guarantee",
        ty: TyAtom(Atom::U32),
    },
];

const SURFACE_DOMAIN_CONTRACT_FIELDS: &[PortableBuiltinField] = &[PortableBuiltinField {
    name: "material",
    ty: TyAtom(Atom::Bool),
}];

const PARTICIPANT_DOMAIN_CONTRACT_FIELDS: &[PortableBuiltinField] = &[
    PortableBuiltinField {
        name: "radiance",
        ty: TyAtom(Atom::Bool),
    },
    PortableBuiltinField {
        name: "media",
        ty: TyAtom(Atom::Bool),
    },
];

const TRACE_QUERY_FIELDS: &[PortableBuiltinField] = &[
    PortableBuiltinField {
        name: "capture",
        ty: TyNamed("ShapeCapture"),
    },
    PortableBuiltinField {
        name: "origin",
        ty: TyAtom(Atom::Vec3),
    },
    PortableBuiltinField {
        name: "direction",
        ty: TyAtom(Atom::Vec3),
    },
    PortableBuiltinField {
        name: "max_distance",
        ty: TyAtom(Atom::F32),
    },
    PortableBuiltinField {
        name: "min_step",
        ty: TyAtom(Atom::F32),
    },
    PortableBuiltinField {
        name: "hit_epsilon",
        ty: TyAtom(Atom::F32),
    },
    PortableBuiltinField {
        name: "max_steps",
        ty: TyAtom(Atom::I32),
    },
];

const SURFACE_QUERY_FIELDS: &[PortableBuiltinField] = &[
    PortableBuiltinField {
        name: "capture",
        ty: TyNamed("ShapeCapture"),
    },
    PortableBuiltinField {
        name: "hit",
        ty: TyNamed("Hit3"),
    },
];

const POINT_QUERY_FIELDS: &[PortableBuiltinField] = &[PortableBuiltinField {
    name: "point",
    ty: TyAtom(Atom::Vec3),
}];

const POINT_DIRECTION_QUERY_FIELDS: &[PortableBuiltinField] = &[
    PortableBuiltinField {
        name: "point",
        ty: TyAtom(Atom::Vec3),
    },
    PortableBuiltinField {
        name: "direction",
        ty: TyAtom(Atom::Vec3),
    },
];

const RAY_QUERY_FIELDS: &[PortableBuiltinField] = &[
    PortableBuiltinField {
        name: "origin",
        ty: TyAtom(Atom::Vec3),
    },
    PortableBuiltinField {
        name: "direction",
        ty: TyAtom(Atom::Vec3),
    },
    PortableBuiltinField {
        name: "max_distance",
        ty: TyAtom(Atom::F32),
    },
    PortableBuiltinField {
        name: "min_step",
        ty: TyAtom(Atom::F32),
    },
    PortableBuiltinField {
        name: "hit_epsilon",
        ty: TyAtom(Atom::F32),
    },
    PortableBuiltinField {
        name: "max_steps",
        ty: TyAtom(Atom::I32),
    },
];

const SURFACE_FIELDS: &[PortableBuiltinField] = &[
    PortableBuiltinField {
        name: "albedo",
        ty: TyAtom(Atom::Vec3),
    },
    PortableBuiltinField {
        name: "roughness",
        ty: TyAtom(Atom::F32),
    },
    PortableBuiltinField {
        name: "metalness",
        ty: TyAtom(Atom::F32),
    },
    PortableBuiltinField {
        name: "clearcoat",
        ty: TyAtom(Atom::F32),
    },
    PortableBuiltinField {
        name: "clearcoat_roughness",
        ty: TyAtom(Atom::F32),
    },
    PortableBuiltinField {
        name: "sheen",
        ty: TyAtom(Atom::F32),
    },
    PortableBuiltinField {
        name: "emissive",
        ty: TyAtom(Atom::Vec3),
    },
];

const MEDIUM_FIELDS: &[PortableBuiltinField] = &[
    PortableBuiltinField {
        name: "density",
        ty: TyAtom(Atom::F32),
    },
    PortableBuiltinField {
        name: "emission",
        ty: TyAtom(Atom::Vec3),
    },
    PortableBuiltinField {
        name: "anisotropy",
        ty: TyAtom(Atom::F32),
    },
];

const ACTOR_HANDLE_FIELDS: &[PortableBuiltinField] = &[
    PortableBuiltinField {
        name: "id",
        ty: TyAtom(Atom::U32),
    },
    PortableBuiltinField {
        name: "generation",
        ty: TyAtom(Atom::U32),
    },
];

const PAYLOAD_FIELDS: &[PortableBuiltinField] = &[
    PortableBuiltinField {
        name: "entity_id",
        ty: TyAtom(Atom::U32),
    },
    PortableBuiltinField {
        name: "material_id",
        ty: TyAtom(Atom::U32),
    },
    PortableBuiltinField {
        name: "actor",
        ty: TyNamed("ActorHandle"),
    },
];

const HIT3_FIELDS: &[PortableBuiltinField] = &[
    PortableBuiltinField {
        name: "hit",
        ty: TyAtom(Atom::Bool),
    },
    PortableBuiltinField {
        name: "distance",
        ty: TyAtom(Atom::F32),
    },
    PortableBuiltinField {
        name: "position",
        ty: TyAtom(Atom::Vec3),
    },
    PortableBuiltinField {
        name: "normal",
        ty: TyAtom(Atom::Vec3),
    },
    PortableBuiltinField {
        name: "local_position",
        ty: TyAtom(Atom::Vec3),
    },
    PortableBuiltinField {
        name: "local_normal",
        ty: TyAtom(Atom::Vec3),
    },
    PortableBuiltinField {
        name: "shading_frame",
        ty: TyNamed("Transform3"),
    },
    PortableBuiltinField {
        name: "steps",
        ty: TyAtom(Atom::I32),
    },
    PortableBuiltinField {
        name: "feature_id",
        ty: TyAtom(Atom::U32),
    },
    PortableBuiltinField {
        name: "instance_id",
        ty: TyAtom(Atom::U32),
    },
    PortableBuiltinField {
        name: "repeat_id",
        ty: TyAtom(Atom::U32),
    },
    PortableBuiltinField {
        name: "root_shape_id",
        ty: TyAtom(Atom::U32),
    },
    PortableBuiltinField {
        name: "payload",
        ty: TyNamed("Payload"),
    },
];

const SUPPORT3_FIELDS: &[PortableBuiltinField] = &[PortableBuiltinField {
    name: "bounds",
    ty: TyNamed("Bounds3"),
}];

const CONTACT_FIELDS: &[PortableBuiltinField] = &[
    PortableBuiltinField {
        name: "hit",
        ty: TyAtom(Atom::Bool),
    },
    PortableBuiltinField {
        name: "position",
        ty: TyAtom(Atom::Vec3),
    },
    PortableBuiltinField {
        name: "normal",
        ty: TyAtom(Atom::Vec3),
    },
    PortableBuiltinField {
        name: "penetration",
        ty: TyAtom(Atom::F32),
    },
    PortableBuiltinField {
        name: "payload",
        ty: TyNamed("Payload"),
    },
];

const LIGHT_FIELDS: &[PortableBuiltinField] = &[
    PortableBuiltinField {
        name: "position",
        ty: TyAtom(Atom::Vec3),
    },
    PortableBuiltinField {
        name: "direction",
        ty: TyAtom(Atom::Vec3),
    },
    PortableBuiltinField {
        name: "intensity",
        ty: TyAtom(Atom::Vec3),
    },
    PortableBuiltinField {
        name: "range",
        ty: TyAtom(Atom::F32),
    },
];

const CAMERA_FIELDS: &[PortableBuiltinField] = &[
    PortableBuiltinField {
        name: "position",
        ty: TyAtom(Atom::Vec3),
    },
    PortableBuiltinField {
        name: "forward",
        ty: TyAtom(Atom::Vec3),
    },
    PortableBuiltinField {
        name: "up",
        ty: TyAtom(Atom::Vec3),
    },
    PortableBuiltinField {
        name: "vertical_fov_degrees",
        ty: TyAtom(Atom::F32),
    },
];

const BUILTIN_RECORDS: &[PortableBuiltinRecord] = &[
    PortableBuiltinRecord {
        name: "Bounds2",
        function_name: Some("bounds2"),
        constructible: true,
        fields: BOUNDS2_FIELDS,
    },
    PortableBuiltinRecord {
        name: "Bounds3",
        function_name: Some("bounds3"),
        constructible: true,
        fields: BOUNDS3_FIELDS,
    },
    PortableBuiltinRecord {
        name: "Ray3",
        function_name: Some("ray3"),
        constructible: true,
        fields: RAY3_FIELDS,
    },
    PortableBuiltinRecord {
        name: "FieldCapture",
        function_name: None,
        constructible: false,
        fields: SCENE_CAPTURE_FIELDS,
    },
    PortableBuiltinRecord {
        name: "ShapeCapture",
        function_name: None,
        constructible: false,
        fields: SCENE_CAPTURE_FIELDS,
    },
    PortableBuiltinRecord {
        name: "RegionCapture",
        function_name: None,
        constructible: false,
        fields: SCENE_CAPTURE_FIELDS,
    },
    PortableBuiltinRecord {
        name: "SpatialDomainContract",
        function_name: None,
        constructible: false,
        fields: SPATIAL_DOMAIN_CONTRACT_FIELDS,
    },
    PortableBuiltinRecord {
        name: "SurfaceDomainContract",
        function_name: None,
        constructible: false,
        fields: SURFACE_DOMAIN_CONTRACT_FIELDS,
    },
    PortableBuiltinRecord {
        name: "ParticipantDomainContract",
        function_name: None,
        constructible: false,
        fields: PARTICIPANT_DOMAIN_CONTRACT_FIELDS,
    },
    PortableBuiltinRecord {
        name: "SceneDomain",
        function_name: None,
        constructible: false,
        fields: &[
            PortableBuiltinField {
                name: "scene_id",
                ty: TyAtom(Atom::U32),
            },
            PortableBuiltinField {
                name: "spatial",
                ty: TyNamed("SpatialDomainContract"),
            },
            PortableBuiltinField {
                name: "surface",
                ty: TyNamed("SurfaceDomainContract"),
            },
            PortableBuiltinField {
                name: "participants",
                ty: TyNamed("ParticipantDomainContract"),
            },
        ],
    },
    PortableBuiltinRecord {
        name: "DispatchBackend",
        function_name: None,
        constructible: false,
        fields: DISPATCH_BACKEND_FIELDS,
    },
    PortableBuiltinRecord {
        name: "PointQuery",
        function_name: Some("point_query"),
        constructible: true,
        fields: POINT_QUERY_FIELDS,
    },
    PortableBuiltinRecord {
        name: "PointDirectionQuery",
        function_name: Some("point_direction_query"),
        constructible: true,
        fields: POINT_DIRECTION_QUERY_FIELDS,
    },
    PortableBuiltinRecord {
        name: "RayQuery",
        function_name: Some("ray_query"),
        constructible: true,
        fields: RAY_QUERY_FIELDS,
    },
    PortableBuiltinRecord {
        name: "DistanceResult",
        function_name: Some("distance_result"),
        constructible: true,
        fields: DISTANCE_RESULT_FIELDS,
    },
    PortableBuiltinRecord {
        name: "NormalResult",
        function_name: Some("normal_result"),
        constructible: true,
        fields: NORMAL_RESULT_FIELDS,
    },
    PortableBuiltinRecord {
        name: "OcclusionResult",
        function_name: Some("occlusion_result"),
        constructible: true,
        fields: OCCLUSION_RESULT_FIELDS,
    },
    PortableBuiltinRecord {
        name: "Transform3",
        function_name: Some("transform3"),
        constructible: true,
        fields: TRANSFORM3_FIELDS,
    },
    PortableBuiltinRecord {
        name: "Surface",
        function_name: None,
        constructible: true,
        fields: SURFACE_FIELDS,
    },
    PortableBuiltinRecord {
        name: "Medium",
        function_name: None,
        constructible: true,
        fields: MEDIUM_FIELDS,
    },
    PortableBuiltinRecord {
        name: "Payload",
        function_name: None,
        constructible: true,
        fields: PAYLOAD_FIELDS,
    },
    PortableBuiltinRecord {
        name: "Hit3",
        function_name: None,
        constructible: true,
        fields: HIT3_FIELDS,
    },
    PortableBuiltinRecord {
        name: "Support3",
        function_name: None,
        constructible: true,
        fields: SUPPORT3_FIELDS,
    },
    PortableBuiltinRecord {
        name: "Contact",
        function_name: None,
        constructible: true,
        fields: CONTACT_FIELDS,
    },
    PortableBuiltinRecord {
        name: "Light",
        function_name: None,
        constructible: true,
        fields: LIGHT_FIELDS,
    },
    PortableBuiltinRecord {
        name: "Camera",
        function_name: None,
        constructible: true,
        fields: CAMERA_FIELDS,
    },
    PortableBuiltinRecord {
        name: "ActorHandle",
        function_name: None,
        constructible: true,
        fields: ACTOR_HANDLE_FIELDS,
    },
];

const INTERNAL_BUILTIN_RECORDS: &[PortableBuiltinRecord] = &[
    PortableBuiltinRecord {
        name: "TraceQuery",
        function_name: None,
        constructible: false,
        fields: TRACE_QUERY_FIELDS,
    },
    PortableBuiltinRecord {
        name: "SurfaceQuery",
        function_name: None,
        constructible: false,
        fields: SURFACE_QUERY_FIELDS,
    },
];

pub const BUILTIN_HELPER_FUNCTIONS: &[&str] = &[
    "transform3_identity",
    "bounds2_center",
    "bounds2_size",
    "bounds3_center",
    "bounds3_size",
    "transform_point",
    "transform_vector",
    "transform_normal",
    "compose_transform3",
    "inverse_transform3",
    "capture",
    "circle2",
    "rect2",
    "rounded_rect2",
    "capsule2",
    "segment2",
    "polygon2",
    "polyline2",
    "field_translate_point",
    "field_rotate_point",
    "field_uniform_scale_point",
    "field_affine_transform_point",
    "field_warp_point",
    "field_repeat_linear_point",
    "field_repeat_grid_point",
    "field_radial_repeat_point",
    "field_mirror_array_point",
    "field_instance_array_point",
    "field_sweep_coords",
    "field_smooth_union",
    "field_smooth_intersection",
    "field_smooth_subtract",
    "field_bend_point",
    "field_twist_point",
    "field_taper_point",
    "field_displace_point",
    "field_union",
    "field_intersection",
    "field_subtract",
    "__wr_field_distance_capture",
    "__wr_field_normal_capture",
    "__wr_shape_distance_capture",
    "__wr_shape_normal_capture",
    "__wr_scene_trace_capture",
    "__wr_scene_occluded_capture",
    "__wr_scene_surface_capture",
    "__wr_scene_radiance_capture",
    "__wr_scene_medium_capture",
    "__wr_world_distance_capture",
    "__wr_world_normal_capture",
    "__wr_world_trace_capture",
    "__wr_world_occluded_capture",
    "__wr_world_surface_capture",
    "__wr_world_radiance_capture",
    "__wr_world_medium_capture",
    "__wr_field_distance_batch_queries",
    "__wr_shape_distance_batch_queries",
    "__wr_field_normal_batch_queries",
    "__wr_shape_normal_batch_queries",
    "__wr_scene_trace_batch_queries",
    "__wr_scene_surface_batch_queries",
    "__wr_scene_occluded_batch_queries",
    "__wr_scene_trace_queries",
    "__wr_scene_surface_queries",
];

pub const BUILTIN_FIELD_PRIMITIVE_FUNCTIONS: &[&str] = &[
    "sphere",
    "box",
    "capsule",
    "cylinder",
    "plane",
    "torus",
    "rounded_box",
    "ellipsoid",
    "cone",
    "capped_cone",
    "box_frame",
    "slab",
    "triangle_prism",
    "hex_prism",
];

pub fn builtin_records() -> &'static [PortableBuiltinRecord] {
    BUILTIN_RECORDS
}

pub fn builtin_record(name: &str) -> Option<&'static PortableBuiltinRecord> {
    BUILTIN_RECORDS.iter().find(|record| record.name == name)
}

pub(crate) fn internal_builtin_record(name: &str) -> Option<&'static PortableBuiltinRecord> {
    INTERNAL_BUILTIN_RECORDS
        .iter()
        .find(|record| record.name == name)
}

pub(crate) fn any_builtin_record(name: &str) -> Option<&'static PortableBuiltinRecord> {
    builtin_record(name).or_else(|| internal_builtin_record(name))
}

pub(crate) fn all_builtin_records() -> impl Iterator<Item = &'static PortableBuiltinRecord> {
    BUILTIN_RECORDS
        .iter()
        .chain(INTERNAL_BUILTIN_RECORDS.iter())
}

pub fn builtin_record_by_function(name: &str) -> Option<&'static PortableBuiltinRecord> {
    BUILTIN_RECORDS
        .iter()
        .find(|record| record.function_name == Some(name))
}

pub fn builtin_record_is_constructible(name: &str) -> bool {
    builtin_record(name)
        .map(|record| record.constructible)
        .unwrap_or(false)
}

pub fn is_builtin_record_name(name: &str) -> bool {
    builtin_record(name).is_some()
}

pub fn is_builtin_record_function(name: &str) -> bool {
    builtin_record_by_function(name).is_some()
}

pub fn is_builtin_helper_function(name: &str) -> bool {
    BUILTIN_HELPER_FUNCTIONS.contains(&name)
}

pub fn is_builtin_field_primitive_function(name: &str) -> bool {
    BUILTIN_FIELD_PRIMITIVE_FUNCTIONS.contains(&name)
}

fn portable_builtin_atom_abi(atom: PortableBuiltinAtom) -> PortableAbiType {
    match atom {
        PortableBuiltinAtom::Bool => PortableAbiType::Bool,
        PortableBuiltinAtom::I32 => PortableAbiType::I32,
        PortableBuiltinAtom::U32 => PortableAbiType::U32,
        PortableBuiltinAtom::F32 => PortableAbiType::F32,
        PortableBuiltinAtom::Vec2 => PortableAbiType::Vec2,
        PortableBuiltinAtom::Vec3 => PortableAbiType::Vec3,
        PortableBuiltinAtom::Vec4 => PortableAbiType::Vec4,
        PortableBuiltinAtom::Mat3 => PortableAbiType::Mat3,
        PortableBuiltinAtom::Mat4 => PortableAbiType::Mat4,
        PortableBuiltinAtom::Quat => PortableAbiType::Quat,
    }
}

pub fn portable_builtin_type_abi(ty: PortableBuiltinType) -> Option<PortableAbiType> {
    match ty {
        PortableBuiltinType::Atom(atom) => Some(portable_builtin_atom_abi(atom)),
        PortableBuiltinType::Named(name) => portable_builtin_record_abi(name),
    }
}

pub fn portable_builtin_record_abi(name: &str) -> Option<PortableAbiType> {
    let class_id = u32::try_from(
        BUILTIN_RECORDS
            .iter()
            .position(|record| record.name == name)?
            .saturating_add(1),
    )
    .ok()?;
    let record = builtin_record(name)?;
    let mut fields = Vec::with_capacity(record.fields.len());
    for field in record.fields {
        fields.push(PortableStructField {
            name: SmolStr::new(field.name),
            ty: portable_builtin_type_abi(field.ty)?,
        });
    }
    Some(PortableAbiType::Struct {
        name: SmolStr::new(record.name),
        class_id,
        fields,
    })
}

fn portable_any_builtin_type_abi(ty: PortableBuiltinType) -> Option<PortableAbiType> {
    match ty {
        PortableBuiltinType::Atom(atom) => Some(portable_builtin_atom_abi(atom)),
        PortableBuiltinType::Named(name) => portable_any_builtin_record_abi(name),
    }
}

pub(crate) fn portable_any_builtin_record_abi(name: &str) -> Option<PortableAbiType> {
    let class_id = u32::try_from(
        all_builtin_records()
            .position(|record| record.name == name)?
            .saturating_add(1),
    )
    .ok()?;
    let record = any_builtin_record(name)?;
    let mut fields = Vec::with_capacity(record.fields.len());
    for field in record.fields {
        fields.push(PortableStructField {
            name: SmolStr::new(field.name),
            ty: portable_any_builtin_type_abi(field.ty)?,
        });
    }
    Some(PortableAbiType::Struct {
        name: SmolStr::new(record.name),
        class_id,
        fields,
    })
}

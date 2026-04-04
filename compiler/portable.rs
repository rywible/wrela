#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PortableBuiltinAtom {
    Bool,
    I32,
    U32,
    I64,
    U64,
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
        ty: TyAtom(Atom::I64),
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
        ty: TyAtom(Atom::U64),
    },
    PortableBuiltinField {
        name: "epoch",
        ty: TyAtom(Atom::U64),
    },
    PortableBuiltinField {
        name: "root_feature_id",
        ty: TyAtom(Atom::U64),
    },
];

const DISPATCH_BACKEND_FIELDS: &[PortableBuiltinField] = &[PortableBuiltinField {
    name: "id",
    ty: TyAtom(Atom::I64),
}];

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
        ty: TyAtom(Atom::I64),
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

const POINT_QUERY_FIELDS: &[PortableBuiltinField] = &[
    PortableBuiltinField {
        name: "point",
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
        ty: TyAtom(Atom::I64),
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
        ty: TyAtom(Atom::U64),
    },
    PortableBuiltinField {
        name: "generation",
        ty: TyAtom(Atom::U32),
    },
];

const PAYLOAD_FIELDS: &[PortableBuiltinField] = &[
    PortableBuiltinField {
        name: "entity_id",
        ty: TyAtom(Atom::U64),
    },
    PortableBuiltinField {
        name: "material_id",
        ty: TyAtom(Atom::U64),
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
        name: "steps",
        ty: TyAtom(Atom::I64),
    },
    PortableBuiltinField {
        name: "feature_id",
        ty: TyAtom(Atom::U64),
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
        name: "DispatchBackend",
        function_name: None,
        constructible: false,
        fields: DISPATCH_BACKEND_FIELDS,
    },
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
    PortableBuiltinRecord {
        name: "PointQuery",
        function_name: Some("point_query"),
        constructible: true,
        fields: POINT_QUERY_FIELDS,
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
    "repeat_point",
    "field_union",
    "field_intersection",
    "field_subtract",
    "__wr_field_distance_capture",
    "__wr_field_normal_capture",
    "__wr_shape_distance_capture",
    "__wr_shape_normal_capture",
    "__wr_scene_trace_capture",
    "__wr_scene_surface_capture",
    "__wr_scene_trace_queries",
    "__wr_scene_surface_queries",
];

pub const BUILTIN_FIELD_PRIMITIVE_FUNCTIONS: &[&str] =
    &["sphere", "box", "capsule", "cylinder", "plane", "torus"];

pub fn builtin_records() -> &'static [PortableBuiltinRecord] {
    BUILTIN_RECORDS
}

pub fn builtin_record(name: &str) -> Option<&'static PortableBuiltinRecord> {
    BUILTIN_RECORDS.iter().find(|record| record.name == name)
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

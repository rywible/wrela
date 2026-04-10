use crate::query_contract::{QueryContractId, query_contract_bundle, scene_domain_flag_name};
use crate::query_plan::{WorldQueryKind, world_query_contract_id};
use smol_str::SmolStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldQuerySemantics {
    pub query_name: &'static str,
    pub domain_flag: Option<&'static str>,
}

pub fn world_query_semantics(kind: WorldQueryKind) -> WorldQuerySemantics {
    let contract_id = world_query_contract_id(kind);
    let mut semantics = world_query_semantics_for_contract(contract_id);
    if matches!(kind, WorldQueryKind::Trace) {
        semantics.query_name = "trace_world";
    }
    semantics
}

pub(crate) fn world_query_semantics_for_contract(
    contract_id: QueryContractId,
) -> WorldQuerySemantics {
    let Some((descriptor, binding)) = query_contract_bundle(contract_id) else {
        panic!(
            "missing world query contract bundle for '{}'",
            contract_id.as_str()
        );
    };
    WorldQuerySemantics {
        query_name: match descriptor.question {
            crate::query_contract::QueryQuestionId::Nearest => "nearest_world",
            _ => binding.legacy_builtin_name,
        },
        domain_flag: descriptor
            .required_domain_flags
            .first()
            .copied()
            .map(scene_domain_flag_name),
    }
}

pub(crate) trait WorldQueryBackend {
    type Error;

    fn with_world_shapes<F>(
        &mut self,
        kind: WorldQueryKind,
        invalid_message: &'static str,
        emit_shapes: F,
    ) -> Result<(), Self::Error>
    where
        F: FnMut(&mut Self, &[SmolStr]) -> Result<(), Self::Error>;

    fn with_domain_flag<F>(&mut self, kind: WorldQueryKind, enabled: F) -> Result<(), Self::Error>
    where
        F: FnOnce(&mut Self) -> Result<(), Self::Error>;
}

pub(crate) trait WorldDistanceBackend {
    type Error;

    fn init_world_distance(&mut self) -> Result<(), Self::Error>;
    fn accumulate_world_distance_shape(&mut self, shape: &SmolStr) -> Result<(), Self::Error>;
}

pub(crate) trait WorldTraceBackend {
    type Error;

    fn init_world_trace(&mut self) -> Result<(), Self::Error>;
    fn consider_world_trace_shape(&mut self, shape: &SmolStr) -> Result<(), Self::Error>;
}

pub(crate) trait WorldSurfaceBackend {
    type Error;

    fn init_world_surface(&mut self) -> Result<(), Self::Error>;
    fn consider_world_surface_shape(&mut self, shape: &SmolStr) -> Result<(), Self::Error>;
}

pub(crate) trait WorldRadianceBackend {
    type Error;

    fn init_world_radiance(&mut self) -> Result<(), Self::Error>;
    fn accumulate_world_radiance_shape(&mut self, shape: &SmolStr) -> Result<(), Self::Error>;
}

pub(crate) trait WorldMediumBackend {
    type Error;

    fn init_world_medium(&mut self) -> Result<(), Self::Error>;
    fn accumulate_world_medium_shape(&mut self, shape: &SmolStr) -> Result<(), Self::Error>;
}

pub(crate) trait WorldNormalBackend {
    type Error;
    type Point: Clone;
    type Distance;
    type Normal;

    fn base_point(&mut self) -> Result<Self::Point, Self::Error>;
    fn offset_point(
        &mut self,
        point: &Self::Point,
        axis: usize,
        delta: f32,
    ) -> Result<Self::Point, Self::Error>;
    fn sample_world_distance(&mut self, point: Self::Point) -> Result<Self::Distance, Self::Error>;
    fn subtract_distance(
        &mut self,
        positive: Self::Distance,
        negative: Self::Distance,
    ) -> Result<Self::Distance, Self::Error>;
    fn compose_normal(
        &mut self,
        x: Self::Distance,
        y: Self::Distance,
        z: Self::Distance,
    ) -> Result<Self::Normal, Self::Error>;
    fn normalize_normal(&mut self, normal: Self::Normal) -> Result<Self::Normal, Self::Error>;
}

pub(crate) fn execute_world_distance<B: WorldDistanceBackend>(
    backend: &mut B,
) -> Result<(), <B as WorldDistanceBackend>::Error>
where
    B: WorldQueryBackend<Error = <B as WorldDistanceBackend>::Error>,
{
    backend.init_world_distance()?;
    backend.with_world_shapes(
        WorldQueryKind::Distance,
        "distance_world requires a capture created from a region declaration",
        |backend, shapes| walk_world_distance_shapes(backend, shapes),
    )
}

pub(crate) fn walk_world_distance_shapes<B: WorldDistanceBackend>(
    backend: &mut B,
    shapes: &[SmolStr],
) -> Result<(), B::Error> {
    for shape in shapes {
        backend.accumulate_world_distance_shape(shape)?;
    }
    Ok(())
}

pub(crate) fn execute_world_ray<B: WorldTraceBackend>(
    backend: &mut B,
    kind: WorldQueryKind,
    invalid_message: &'static str,
) -> Result<(), <B as WorldTraceBackend>::Error>
where
    B: WorldQueryBackend<Error = <B as WorldTraceBackend>::Error>,
{
    backend.init_world_trace()?;
    backend.with_world_shapes(kind, invalid_message, |backend, shapes| {
        walk_world_trace_shapes(backend, shapes)
    })
}

pub(crate) fn walk_world_trace_shapes<B: WorldTraceBackend>(
    backend: &mut B,
    shapes: &[SmolStr],
) -> Result<(), B::Error> {
    for shape in shapes {
        backend.consider_world_trace_shape(shape)?;
    }
    Ok(())
}

pub(crate) fn execute_world_normal<B: WorldNormalBackend>(
    backend: &mut B,
) -> Result<B::Normal, B::Error> {
    let point = backend.base_point()?;
    let px = backend.offset_point(&point, 0, 0.001)?;
    let nx = backend.offset_point(&point, 0, -0.001)?;
    let py = backend.offset_point(&point, 1, 0.001)?;
    let ny = backend.offset_point(&point, 1, -0.001)?;
    let pz = backend.offset_point(&point, 2, 0.001)?;
    let nz = backend.offset_point(&point, 2, -0.001)?;

    let dx_pos = backend.sample_world_distance(px)?;
    let dx_neg = backend.sample_world_distance(nx)?;
    let dx = backend.subtract_distance(dx_pos, dx_neg)?;
    let dy_pos = backend.sample_world_distance(py)?;
    let dy_neg = backend.sample_world_distance(ny)?;
    let dy = backend.subtract_distance(dy_pos, dy_neg)?;
    let dz_pos = backend.sample_world_distance(pz)?;
    let dz_neg = backend.sample_world_distance(nz)?;
    let dz = backend.subtract_distance(dz_pos, dz_neg)?;
    let normal = backend.compose_normal(dx, dy, dz)?;
    backend.normalize_normal(normal)
}

pub(crate) fn execute_world_surface<B: WorldSurfaceBackend>(
    backend: &mut B,
) -> Result<(), <B as WorldSurfaceBackend>::Error>
where
    B: WorldQueryBackend<Error = <B as WorldSurfaceBackend>::Error>,
{
    backend.init_world_surface()?;
    backend.with_domain_flag(WorldQueryKind::Surface, |backend| {
        backend.with_world_shapes(
            WorldQueryKind::Surface,
            "surface_world requires a capture created from a region declaration",
            |backend, shapes| walk_world_surface_shapes(backend, shapes),
        )
    })
}

pub(crate) fn walk_world_surface_shapes<B: WorldSurfaceBackend>(
    backend: &mut B,
    shapes: &[SmolStr],
) -> Result<(), B::Error> {
    for shape in shapes {
        backend.consider_world_surface_shape(shape)?;
    }
    Ok(())
}

pub(crate) fn execute_world_radiance<B: WorldRadianceBackend>(
    backend: &mut B,
) -> Result<(), <B as WorldRadianceBackend>::Error>
where
    B: WorldQueryBackend<Error = <B as WorldRadianceBackend>::Error>,
{
    backend.init_world_radiance()?;
    backend.with_domain_flag(WorldQueryKind::Radiance, |backend| {
        backend.with_world_shapes(
            WorldQueryKind::Radiance,
            "radiance_world requires a capture created from a region declaration",
            |backend, shapes| walk_world_radiance_shapes(backend, shapes),
        )
    })
}

pub(crate) fn walk_world_radiance_shapes<B: WorldRadianceBackend>(
    backend: &mut B,
    shapes: &[SmolStr],
) -> Result<(), B::Error> {
    for shape in shapes {
        backend.accumulate_world_radiance_shape(shape)?;
    }
    Ok(())
}

pub(crate) fn execute_world_medium<B: WorldMediumBackend>(
    backend: &mut B,
) -> Result<(), <B as WorldMediumBackend>::Error>
where
    B: WorldQueryBackend<Error = <B as WorldMediumBackend>::Error>,
{
    backend.init_world_medium()?;
    backend.with_domain_flag(WorldQueryKind::Medium, |backend| {
        backend.with_world_shapes(
            WorldQueryKind::Medium,
            "medium_world requires a capture created from a region declaration",
            |backend, shapes| walk_world_medium_shapes(backend, shapes),
        )
    })
}

pub(crate) fn walk_world_medium_shapes<B: WorldMediumBackend>(
    backend: &mut B,
    shapes: &[SmolStr],
) -> Result<(), B::Error> {
    for shape in shapes {
        backend.accumulate_world_medium_shape(shape)?;
    }
    Ok(())
}

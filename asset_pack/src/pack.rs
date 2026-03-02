use std::collections::{BTreeMap, BTreeSet};

use crate::types::{
    AssetChunk, AssetPackManifestV3, ConvergenceStage, ResidencyClass, WorldChunk,
    WorldChunkManifestV2,
};

const ASSET_PACK_SCHEMA_VERSION: u32 = 4;
const WORLD_CHUNK_SCHEMA_VERSION: u32 = 3;
const ASSET_PACK_KIND: &str = "asset_pack_manifest_v4";
const WORLD_CHUNK_KIND: &str = "world_chunk_manifest_v3";

fn convergence_stage_rank(stage: ConvergenceStage) -> u8 {
    match stage {
        ConvergenceStage::Bootstrap => 0,
        ConvergenceStage::Stream => 1,
        ConvergenceStage::Refine => 2,
        ConvergenceStage::Converged => 3,
    }
}

fn residency_class_rank(class: ResidencyClass) -> u8 {
    match class {
        ResidencyClass::Core => 0,
        ResidencyClass::Hot => 1,
        ResidencyClass::Warm => 2,
        ResidencyClass::Cold => 3,
    }
}

fn is_hex_hash(value: &str) -> bool {
    value.len() >= 16 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn bounds_are_valid(bounds: &crate::types::LodBounds) -> bool {
    bounds.min[0] <= bounds.max[0]
        && bounds.min[1] <= bounds.max[1]
        && bounds.min[2] <= bounds.max[2]
}

pub fn build_pack_index(pack: &AssetPackManifestV3) -> Result<BTreeMap<String, usize>, String> {
    let mut index = BTreeMap::new();

    for (position, chunk) in pack.chunks.iter().enumerate() {
        if index.insert(chunk.id.clone(), position).is_some() {
            return Err(format!("duplicate asset chunk id: {}", chunk.id));
        }
    }

    Ok(index)
}

pub fn validate_asset_pack(pack: &AssetPackManifestV3) -> Result<(), String> {
    if pack.schema_version != ASSET_PACK_SCHEMA_VERSION {
        return Err(format!(
            "asset pack schema version mismatch: expected {} but found {}",
            ASSET_PACK_SCHEMA_VERSION, pack.schema_version
        ));
    }
    if pack.kind != ASSET_PACK_KIND {
        return Err(format!(
            "asset pack kind mismatch: expected '{}' but found '{}'",
            ASSET_PACK_KIND, pack.kind
        ));
    }
    if pack.pack_id.trim().is_empty() {
        return Err("asset pack id must not be empty".to_string());
    }
    if pack.chunks.is_empty() {
        return Err("asset pack must include at least one chunk".to_string());
    }
    if pack.partitions.is_empty() {
        return Err("asset pack must include at least one partition".to_string());
    }

    for window in pack.chunks.windows(2) {
        if window[0].id >= window[1].id {
            return Err(
                "asset chunks must be sorted by id in strict ascending deterministic order"
                    .to_string(),
            );
        }
    }

    let index = build_pack_index(pack)?;
    let mut chunk_partition_assignment = BTreeMap::<&str, u32>::new();
    let mut expected_partition_id = 0u32;
    let mut partition_budget_total = 0u64;

    for chunk in &pack.chunks {
        if !matches!(chunk.residency_priority.as_str(), "high" | "normal" | "low") {
            return Err(format!(
                "invalid residency_priority '{}' for chunk '{}' (expected one of: high, normal, low)",
                chunk.residency_priority, chunk.id
            ));
        }
        if !is_hex_hash(chunk.deterministic_hash.as_str()) {
            return Err(format!(
                "chunk '{}' must include deterministic_hash as a hexadecimal digest",
                chunk.id
            ));
        }
        if chunk.conditioning_evidence.pipeline.trim().is_empty()
            || chunk.conditioning_evidence.source_hash.trim().is_empty()
            || chunk
                .conditioning_evidence
                .deterministic_hash
                .trim()
                .is_empty()
            || chunk.conditioning_evidence.steps.is_empty()
        {
            return Err(format!(
                "chunk '{}' is missing required conditioning_evidence fields",
                chunk.id
            ));
        }
        for window in chunk.conditioning_evidence.steps.windows(2) {
            if window[0] >= window[1] {
                return Err(format!(
                    "conditioning_evidence.steps for chunk '{}' must be sorted and unique in ascending order",
                    chunk.id
                ));
            }
        }
        if !is_hex_hash(chunk.conditioning_evidence.deterministic_hash.as_str()) {
            return Err(format!(
                "chunk '{}' conditioning_evidence.deterministic_hash must be hexadecimal",
                chunk.id
            ));
        }
        if chunk.compression.codec.trim().is_empty()
            || chunk.compression.uncompressed_bytes == 0
            || chunk.compression.compressed_bytes == 0
            || chunk.compression.block_bytes == 0
        {
            return Err(format!(
                "chunk '{}' is missing required compression metadata",
                chunk.id
            ));
        }
        if chunk.compression.compressed_bytes != chunk.bytes {
            return Err(format!(
                "chunk '{}' bytes {} must match compression.compressed_bytes {}",
                chunk.id, chunk.bytes, chunk.compression.compressed_bytes
            ));
        }
        let expected_ratio = ((u128::from(chunk.compression.compressed_bytes) * 1000)
            / u128::from(chunk.compression.uncompressed_bytes)) as u32;
        if chunk.compression.ratio_milli != expected_ratio {
            return Err(format!(
                "chunk '{}' compression ratio_milli {} must equal computed ratio {}",
                chunk.id, chunk.compression.ratio_milli, expected_ratio
            ));
        }
        let expected_total_tiles = u128::from(chunk.tile.tile_rows)
            .saturating_mul(u128::from(chunk.tile.tile_columns))
            .saturating_mul(u128::from(chunk.tile.tile_layers));
        if chunk.tile.tile_width == 0
            || chunk.tile.tile_height == 0
            || chunk.tile.tile_layers == 0
            || chunk.tile.tile_rows == 0
            || chunk.tile.tile_columns == 0
            || chunk.tile.total_tiles == 0
            || chunk.tile.tile_format.trim().is_empty()
        {
            return Err(format!(
                "chunk '{}' is missing required tile metadata",
                chunk.id
            ));
        }
        if expected_total_tiles != u128::from(chunk.tile.total_tiles) {
            return Err(format!(
                "chunk '{}' tile metadata mismatch: total_tiles {} must equal rows*columns*layers {}",
                chunk.id, chunk.tile.total_tiles, expected_total_tiles
            ));
        }
        if u64::from(chunk.tile.total_tiles) > chunk.compression.uncompressed_bytes {
            return Err(format!(
                "chunk '{}' tile metadata invalid: total_tiles {} exceeds uncompressed_bytes {}",
                chunk.id, chunk.tile.total_tiles, chunk.compression.uncompressed_bytes
            ));
        }
        let stage_rank = convergence_stage_rank(chunk.convergence_stage);
        let class_rank = residency_class_rank(chunk.residency_class);
        if class_rank < stage_rank {
            return Err(format!(
                "chunk '{}' residency_class {:?} is incompatible with convergence_stage {:?}",
                chunk.id, chunk.residency_class, chunk.convergence_stage
            ));
        }
        if chunk.lod.source_asset_id.trim().is_empty()
            || chunk.lod.source_hash.trim().is_empty()
            || chunk.lod.max_lod == 0
            || !bounds_are_valid(&chunk.lod.bounds)
        {
            return Err(format!(
                "chunk '{}' is missing required lod lineage/bounds metadata",
                chunk.id
            ));
        }
        for dependency_id in &chunk.dependencies {
            if !index.contains_key(dependency_id) {
                return Err(format!(
                    "missing dependency id '{}' referenced by chunk '{}'",
                    dependency_id, chunk.id
                ));
            }
        }
        for window in chunk.dependencies.windows(2) {
            if window[0] >= window[1] {
                return Err(format!(
                    "dependencies for chunk '{}' must be sorted and unique in ascending order",
                    chunk.id
                ));
            }
        }
    }

    for partition in &pack.partitions {
        if partition.id != expected_partition_id {
            return Err(format!(
                "asset partitions must use deterministic contiguous ids starting at 0; expected {} but found {}",
                expected_partition_id, partition.id
            ));
        }
        expected_partition_id = expected_partition_id.saturating_add(1);
        if partition.chunk_ids.is_empty() {
            return Err(format!(
                "asset partition {} must reference at least one chunk",
                partition.id
            ));
        }
        for window in partition.chunk_ids.windows(2) {
            if window[0] >= window[1] {
                return Err(format!(
                    "chunk_ids for asset partition {} must be sorted and unique in ascending order",
                    partition.id
                ));
            }
        }

        let mut partition_bytes = 0u64;
        let mut required_prefetch_budget = 0u32;
        for chunk_id in &partition.chunk_ids {
            let chunk_position = index.get(chunk_id).ok_or_else(|| {
                format!(
                    "asset partition {} references missing chunk id '{}'",
                    partition.id, chunk_id
                )
            })?;
            let chunk = &pack.chunks[*chunk_position];
            if chunk_partition_assignment
                .insert(chunk.id.as_str(), partition.id)
                .is_some()
            {
                return Err(format!(
                    "asset chunk '{}' assigned to multiple partitions",
                    chunk.id
                ));
            }
            partition_bytes = partition_bytes.saturating_add(chunk.bytes);
            required_prefetch_budget =
                required_prefetch_budget.saturating_add(required_prefetch_weight(chunk));
        }

        if partition.residency_budget_bytes < partition_bytes {
            return Err(format!(
                "asset partition {} residency_budget_bytes {} is below required bytes {}",
                partition.id, partition.residency_budget_bytes, partition_bytes
            ));
        }
        if partition.prefetch_budget < required_prefetch_budget {
            return Err(format!(
                "asset partition {} prefetch_budget {} is below required budget {}",
                partition.id, partition.prefetch_budget, required_prefetch_budget
            ));
        }
        partition_budget_total =
            partition_budget_total.saturating_add(partition.residency_budget_bytes);
    }

    if chunk_partition_assignment.len() != pack.chunks.len() {
        return Err(format!(
            "asset partition coverage mismatch: {} chunks assigned but manifest contains {} chunks",
            chunk_partition_assignment.len(),
            pack.chunks.len()
        ));
    }

    if pack.streaming_budget_bytes != partition_budget_total {
        return Err(format!(
            "asset streaming_budget_bytes {} must equal total partition residency budgets {}",
            pack.streaming_budget_bytes, partition_budget_total
        ));
    }

    Ok(())
}

pub fn validate_world_manifest(
    pack: &AssetPackManifestV3,
    world: &WorldChunkManifestV2,
) -> Result<(), String> {
    if world.schema_version != WORLD_CHUNK_SCHEMA_VERSION {
        return Err(format!(
            "world chunk schema version mismatch: expected {} but found {}",
            WORLD_CHUNK_SCHEMA_VERSION, world.schema_version
        ));
    }
    if world.kind != WORLD_CHUNK_KIND {
        return Err(format!(
            "world chunk kind mismatch: expected '{}' but found '{}'",
            WORLD_CHUNK_KIND, world.kind
        ));
    }
    if world.world_id.trim().is_empty() {
        return Err("world id must not be empty".to_string());
    }
    if world.chunks.is_empty() {
        return Err("world manifest must include at least one chunk".to_string());
    }

    let pack_index = build_pack_index(pack)?;
    let pack_chunk_partition_index = build_chunk_partition_index(pack)?;
    let pack_prefetch_budget_index = build_prefetch_budget_index(pack);
    let pack_partition_ids = pack
        .partitions
        .iter()
        .map(|partition| partition.id)
        .collect::<BTreeSet<_>>();
    let mut world_chunk_ids = BTreeSet::new();
    let mut world_partition_index = BTreeMap::<&str, u32>::new();

    if world.partitions.len() != world.chunks.len() {
        return Err(format!(
            "world partition metadata must assign every world chunk exactly once (partitions={}, chunks={})",
            world.partitions.len(),
            world.chunks.len()
        ));
    }
    for window in world.partitions.windows(2) {
        if window[0].world_chunk_id >= window[1].world_chunk_id {
            return Err(
                "world partition metadata must be sorted by world_chunk_id in strict ascending deterministic order"
                    .to_string(),
            );
        }
    }
    for assignment in &world.partitions {
        if !pack_partition_ids.contains(&assignment.partition_id) {
            return Err(format!(
                "world chunk '{}' references unknown asset partition id {}",
                assignment.world_chunk_id, assignment.partition_id
            ));
        }
        if world_partition_index
            .insert(assignment.world_chunk_id.as_str(), assignment.partition_id)
            .is_some()
        {
            return Err(format!(
                "duplicate world partition assignment for world chunk '{}'",
                assignment.world_chunk_id
            ));
        }
    }

    for window in world.chunks.windows(2) {
        if window[0].id >= window[1].id {
            return Err(
                "world chunks must be sorted by id in strict ascending deterministic order"
                    .to_string(),
            );
        }
    }
    for world_chunk in &world.chunks {
        if !world_chunk_ids.insert(world_chunk.id.as_str()) {
            return Err(format!("duplicate world chunk id: {}", world_chunk.id));
        }
        let world_partition_id = world_partition_index
            .get(world_chunk.id.as_str())
            .ok_or_else(|| {
                format!(
                    "missing partition assignment for world chunk '{}'",
                    world_chunk.id
                )
            })?;
        for window in world_chunk.asset_chunk_ids.windows(2) {
            if window[0] >= window[1] {
                return Err(format!(
                    "asset_chunk_ids for world chunk '{}' must be sorted and unique in ascending order",
                    world_chunk.id
                ));
            }
        }
        for window in world_chunk.prefetch_neighbors.windows(2) {
            if window[0] >= window[1] {
                return Err(format!(
                    "prefetch_neighbors for world chunk '{}' must be sorted and unique in ascending order",
                    world_chunk.id
                ));
            }
        }
        if world_chunk
            .prefetch_neighbors
            .iter()
            .any(|neighbor| neighbor == &world_chunk.id)
        {
            return Err(format!(
                "world chunk '{}' must not include itself in prefetch_neighbors",
                world_chunk.id
            ));
        }
        let prefetch_budget_for_partition = *pack_prefetch_budget_index
            .get(world_partition_id)
            .ok_or_else(|| {
                format!(
                    "missing prefetch budget for asset partition {}",
                    world_partition_id
                )
            })?;
        if world_chunk.prefetch_neighbors.len() > prefetch_budget_for_partition as usize {
            return Err(format!(
                "world chunk '{}' prefetch neighbor count {} exceeds partition {} prefetch_budget {}",
                world_chunk.id,
                world_chunk.prefetch_neighbors.len(),
                world_partition_id,
                prefetch_budget_for_partition
            ));
        }

        validate_world_chunk_refinement_sequence(
            world_chunk,
            *world_partition_id,
            &pack_index,
            &pack_chunk_partition_index,
        )?;

        for asset_chunk_id in &world_chunk.asset_chunk_ids {
            if !pack_index.contains_key(asset_chunk_id) {
                return Err(format!(
                    "missing asset chunk id '{}' referenced by world chunk '{}'",
                    asset_chunk_id, world_chunk.id
                ));
            }
            let chunk_partition_id = pack_chunk_partition_index
                .get(asset_chunk_id.as_str())
                .ok_or_else(|| {
                    format!(
                        "missing asset partition assignment for chunk '{}' referenced by world chunk '{}'",
                        asset_chunk_id, world_chunk.id
                    )
                })?;
            if chunk_partition_id != world_partition_id {
                return Err(format!(
                    "world chunk '{}' in partition {} references chunk '{}' from partition {}",
                    world_chunk.id, world_partition_id, asset_chunk_id, chunk_partition_id
                ));
            }
        }
    }

    for world_chunk in &world.chunks {
        for neighbor_id in &world_chunk.prefetch_neighbors {
            if !world_chunk_ids.contains(neighbor_id.as_str()) {
                return Err(format!(
                    "missing neighbor world chunk id '{}' referenced by world chunk '{}'",
                    neighbor_id, world_chunk.id
                ));
            }
            let Some(neighbor_chunk) = world.chunks.iter().find(|chunk| chunk.id == *neighbor_id)
            else {
                return Err(format!(
                    "missing neighbor world chunk id '{}' referenced by world chunk '{}'",
                    neighbor_id, world_chunk.id
                ));
            };
            if !neighbor_chunk
                .prefetch_neighbors
                .iter()
                .any(|candidate| candidate == &world_chunk.id)
            {
                return Err(format!(
                    "prefetch neighbor relationship must be symmetric between '{}' and '{}'",
                    world_chunk.id, neighbor_id
                ));
            }
        }
    }

    Ok(())
}

fn validate_world_chunk_refinement_sequence(
    world_chunk: &WorldChunk,
    world_partition_id: u32,
    pack_index: &BTreeMap<String, usize>,
    pack_chunk_partition_index: &BTreeMap<&str, u32>,
) -> Result<(), String> {
    for window in world_chunk.hlod_asset_chunk_ids.windows(2) {
        if window[0] >= window[1] {
            return Err(format!(
                "hlod_asset_chunk_ids for world chunk '{}' must be sorted and unique in ascending order",
                world_chunk.id
            ));
        }
    }
    for hlod_id in &world_chunk.hlod_asset_chunk_ids {
        if !pack_index.contains_key(hlod_id) {
            return Err(format!(
                "missing HLOD asset chunk id '{}' referenced by world chunk '{}'",
                hlod_id, world_chunk.id
            ));
        }
        let hlod_partition_id = pack_chunk_partition_index
            .get(hlod_id.as_str())
            .ok_or_else(|| {
                format!(
                    "missing asset partition assignment for HLOD chunk '{}' referenced by world chunk '{}'",
                    hlod_id, world_chunk.id
                )
            })?;
        if *hlod_partition_id != world_partition_id {
            return Err(format!(
                "world chunk '{}' in partition {} references HLOD chunk '{}' from partition {}",
                world_chunk.id, world_partition_id, hlod_id, hlod_partition_id
            ));
        }
    }

    if world_chunk.refinement_sequence.is_empty() {
        return Err(format!(
            "world chunk '{}' must include non-empty refinement_sequence",
            world_chunk.id
        ));
    }

    let world_asset_set = world_chunk
        .asset_chunk_ids
        .iter()
        .map(|value| value.as_str())
        .collect::<BTreeSet<_>>();
    let world_hlod_set = world_chunk
        .hlod_asset_chunk_ids
        .iter()
        .map(|value| value.as_str())
        .collect::<BTreeSet<_>>();
    let mut previous_stage_rank: Option<u8> = None;
    let mut previous_assets = BTreeSet::<&str>::new();

    for (step_idx, step) in world_chunk.refinement_sequence.iter().enumerate() {
        for window in step.asset_chunk_ids.windows(2) {
            if window[0] >= window[1] {
                return Err(format!(
                    "refinement_sequence[{step_idx}] asset_chunk_ids for world chunk '{}' must be sorted and unique in ascending order",
                    world_chunk.id
                ));
            }
        }
        for window in step.hlod_asset_chunk_ids.windows(2) {
            if window[0] >= window[1] {
                return Err(format!(
                    "refinement_sequence[{step_idx}] hlod_asset_chunk_ids for world chunk '{}' must be sorted and unique in ascending order",
                    world_chunk.id
                ));
            }
        }

        let stage_rank = convergence_stage_rank(step.stage);
        if let Some(previous) = previous_stage_rank {
            if stage_rank <= previous {
                return Err(format!(
                    "world chunk '{}' refinement_sequence must use strictly increasing stage order",
                    world_chunk.id
                ));
            }
        }
        previous_stage_rank = Some(stage_rank);

        let step_assets = step
            .asset_chunk_ids
            .iter()
            .map(|value| value.as_str())
            .collect::<BTreeSet<_>>();
        if !step_assets.is_subset(&world_asset_set) {
            return Err(format!(
                "world chunk '{}' refinement_sequence[{step_idx}] references asset_chunk_ids outside world chunk coverage",
                world_chunk.id
            ));
        }
        if !previous_assets.is_subset(&step_assets) {
            return Err(format!(
                "world chunk '{}' refinement_sequence must monotonically refine asset coverage without regressions",
                world_chunk.id
            ));
        }
        previous_assets = step_assets;

        let step_hlods = step
            .hlod_asset_chunk_ids
            .iter()
            .map(|value| value.as_str())
            .collect::<BTreeSet<_>>();
        if !step_hlods.is_subset(&world_hlod_set) {
            return Err(format!(
                "world chunk '{}' refinement_sequence[{step_idx}] references HLOD chunks outside hlod_asset_chunk_ids",
                world_chunk.id
            ));
        }
    }

    let first = world_chunk
        .refinement_sequence
        .first()
        .expect("non-empty refinement sequence");
    if first.stage != ConvergenceStage::Bootstrap {
        return Err(format!(
            "world chunk '{}' refinement_sequence must begin at bootstrap stage",
            world_chunk.id
        ));
    }
    if !world_hlod_set.is_empty() {
        let first_hlod_set = first
            .hlod_asset_chunk_ids
            .iter()
            .map(|value| value.as_str())
            .collect::<BTreeSet<_>>();
        if first_hlod_set != world_hlod_set {
            return Err(format!(
                "world chunk '{}' refinement_sequence bootstrap stage must include all hlod_asset_chunk_ids",
                world_chunk.id
            ));
        }
    }

    let last = world_chunk
        .refinement_sequence
        .last()
        .expect("non-empty refinement sequence");
    if last.stage != ConvergenceStage::Converged {
        return Err(format!(
            "world chunk '{}' refinement_sequence must terminate at converged stage",
            world_chunk.id
        ));
    }
    let final_asset_set = last
        .asset_chunk_ids
        .iter()
        .map(|value| value.as_str())
        .collect::<BTreeSet<_>>();
    if final_asset_set != world_asset_set {
        return Err(format!(
            "world chunk '{}' refinement_sequence converged stage must cover every asset_chunk_id",
            world_chunk.id
        ));
    }

    Ok(())
}

fn required_prefetch_weight(chunk: &AssetChunk) -> u32 {
    match chunk.residency_priority.as_str() {
        "high" => 3,
        "normal" => 2,
        "low" => 1,
        _ => 0,
    }
}

fn build_chunk_partition_index(pack: &AssetPackManifestV3) -> Result<BTreeMap<&str, u32>, String> {
    let mut mapping = BTreeMap::new();
    for partition in &pack.partitions {
        for chunk_id in &partition.chunk_ids {
            if mapping.insert(chunk_id.as_str(), partition.id).is_some() {
                return Err(format!(
                    "asset chunk '{}' assigned to multiple partitions",
                    chunk_id
                ));
            }
        }
    }
    Ok(mapping)
}

fn build_prefetch_budget_index(pack: &AssetPackManifestV3) -> BTreeMap<u32, u32> {
    let mut mapping = BTreeMap::new();
    for partition in &pack.partitions {
        mapping.insert(partition.id, partition.prefetch_budget);
    }
    mapping
}

#[cfg(test)]
mod tests {
    use super::{build_pack_index, validate_asset_pack, validate_world_manifest};
    use crate::types::{
        AssetChunk, AssetPackManifestV3, AssetPartition, CompressionMetadata, ConditioningEvidence,
        ConvergenceStage, LodBounds, LodLineage, ResidencyClass, TileMetadata, WorldChunk,
        WorldChunkManifestV2, WorldChunkPartition, WorldChunkRefinementStep,
    };

    fn chunk(id: &str, bytes: u64, residency_priority: &str, dependencies: &[&str]) -> AssetChunk {
        let (residency_class, convergence_stage) = match residency_priority {
            "high" => (ResidencyClass::Core, ConvergenceStage::Bootstrap),
            "normal" => (ResidencyClass::Warm, ConvergenceStage::Stream),
            _ => (ResidencyClass::Cold, ConvergenceStage::Converged),
        };
        AssetChunk {
            id: id.to_string(),
            path: format!("assets/{id}.bin"),
            bytes,
            checksum: 0,
            dependencies: dependencies.iter().map(|value| value.to_string()).collect(),
            residency_priority: residency_priority.to_string(),
            residency_class,
            convergence_stage,
            deterministic_hash: "f001cafe01234567".to_string(),
            conditioning_evidence: ConditioningEvidence {
                pipeline: "asset-conditioning-v2".to_string(),
                source_hash: format!("source-{id}"),
                deterministic_hash: "deadbeef01234567".to_string(),
                steps: vec![
                    "compress".to_string(),
                    "hash".to_string(),
                    "normalize".to_string(),
                ],
            },
            compression: CompressionMetadata {
                codec: "store".to_string(),
                uncompressed_bytes: bytes,
                compressed_bytes: bytes,
                ratio_milli: 1000,
                block_bytes: 4,
            },
            tile: TileMetadata {
                tile_width: 1,
                tile_height: 1,
                tile_layers: 1,
                tile_rows: 1,
                tile_columns: 1,
                total_tiles: 1,
                tile_format: "r8unorm".to_string(),
            },
            lod: LodLineage {
                source_asset_id: id.to_string(),
                source_hash: format!("lod-{id}"),
                max_lod: 1,
                bounds: LodBounds {
                    min: [-1, -1, -1],
                    max: [1, 1, 1],
                },
            },
        }
    }

    fn pack(chunks: Vec<AssetChunk>) -> AssetPackManifestV3 {
        AssetPackManifestV3 {
            schema_version: 4,
            kind: "asset_pack_manifest_v4".to_string(),
            pack_id: "pack-main".to_string(),
            streaming_budget_bytes: 59,
            partitions: vec![
                AssetPartition {
                    id: 0,
                    chunk_ids: vec!["chunk.a".to_string(), "chunk.b".to_string()],
                    residency_budget_bytes: 45,
                    prefetch_budget: 5,
                },
                AssetPartition {
                    id: 1,
                    chunk_ids: vec!["chunk.c".to_string()],
                    residency_budget_bytes: 14,
                    prefetch_budget: 1,
                },
            ],
            chunks,
        }
    }

    fn valid_pack() -> AssetPackManifestV3 {
        pack(vec![
            chunk("chunk.a", 20, "high", &[]),
            chunk("chunk.b", 18, "normal", &["chunk.a"]),
            chunk("chunk.c", 12, "low", &["chunk.b"]),
        ])
    }

    fn valid_world() -> WorldChunkManifestV2 {
        WorldChunkManifestV2 {
            schema_version: 3,
            kind: "world_chunk_manifest_v3".to_string(),
            world_id: "world-1".to_string(),
            partitions: vec![
                WorldChunkPartition {
                    world_chunk_id: "world.chunk.0".to_string(),
                    partition_id: 0,
                },
                WorldChunkPartition {
                    world_chunk_id: "world.chunk.1".to_string(),
                    partition_id: 1,
                },
            ],
            chunks: vec![
                WorldChunk {
                    id: "world.chunk.0".to_string(),
                    asset_chunk_ids: vec!["chunk.a".to_string(), "chunk.b".to_string()],
                    hlod_asset_chunk_ids: vec!["chunk.a".to_string()],
                    prefetch_neighbors: vec!["world.chunk.1".to_string()],
                    refinement_sequence: vec![
                        WorldChunkRefinementStep {
                            stage: ConvergenceStage::Bootstrap,
                            asset_chunk_ids: vec!["chunk.a".to_string()],
                            hlod_asset_chunk_ids: vec!["chunk.a".to_string()],
                        },
                        WorldChunkRefinementStep {
                            stage: ConvergenceStage::Refine,
                            asset_chunk_ids: vec!["chunk.a".to_string(), "chunk.b".to_string()],
                            hlod_asset_chunk_ids: Vec::new(),
                        },
                        WorldChunkRefinementStep {
                            stage: ConvergenceStage::Converged,
                            asset_chunk_ids: vec!["chunk.a".to_string(), "chunk.b".to_string()],
                            hlod_asset_chunk_ids: Vec::new(),
                        },
                    ],
                },
                WorldChunk {
                    id: "world.chunk.1".to_string(),
                    asset_chunk_ids: vec!["chunk.c".to_string()],
                    hlod_asset_chunk_ids: vec!["chunk.c".to_string()],
                    prefetch_neighbors: vec!["world.chunk.0".to_string()],
                    refinement_sequence: vec![
                        WorldChunkRefinementStep {
                            stage: ConvergenceStage::Bootstrap,
                            asset_chunk_ids: vec!["chunk.c".to_string()],
                            hlod_asset_chunk_ids: vec!["chunk.c".to_string()],
                        },
                        WorldChunkRefinementStep {
                            stage: ConvergenceStage::Converged,
                            asset_chunk_ids: vec!["chunk.c".to_string()],
                            hlod_asset_chunk_ids: Vec::new(),
                        },
                    ],
                },
            ],
        }
    }

    #[test]
    fn validate_asset_pack_accepts_valid_v4_contract() {
        let manifest = valid_pack();
        validate_asset_pack(&manifest).expect("valid manifest should pass validation");
    }

    #[test]
    fn validate_asset_pack_rejects_duplicate_chunk_ids() {
        let manifest = pack(vec![
            chunk("chunk.a", 20, "high", &[]),
            chunk("chunk.a", 18, "normal", &[]),
            chunk("chunk.c", 12, "low", &[]),
        ]);

        let error = validate_asset_pack(&manifest).expect_err("expected duplicate id error");

        assert!(
            error.contains(
                "asset chunks must be sorted by id in strict ascending deterministic order"
            )
        );
    }

    #[test]
    fn validate_asset_pack_rejects_missing_dependency() {
        let manifest = pack(vec![
            chunk("chunk.a", 20, "high", &[]),
            chunk("chunk.b", 18, "normal", &["chunk.a"]),
            chunk("chunk.c", 12, "low", &["texture.missing"]),
        ]);

        let error = validate_asset_pack(&manifest).expect_err("expected missing dependency error");

        assert!(
            error.contains("missing dependency id 'texture.missing' referenced by chunk 'chunk.c'")
        );
    }

    #[test]
    fn validate_asset_pack_rejects_nondeterministic_chunk_order() {
        let manifest = pack(vec![
            chunk("chunk.b", 18, "normal", &["chunk.a"]),
            chunk("chunk.a", 20, "high", &[]),
            chunk("chunk.c", 12, "low", &["chunk.b"]),
        ]);

        let error = validate_asset_pack(&manifest)
            .expect_err("expected deterministic chunk ordering failure");

        assert!(
            error.contains(
                "asset chunks must be sorted by id in strict ascending deterministic order"
            ),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn validate_asset_pack_rejects_partition_budget_underflow() {
        let mut manifest = valid_pack();
        manifest.partitions[0].prefetch_budget = 4;

        let error =
            validate_asset_pack(&manifest).expect_err("expected prefetch budget underflow error");

        assert!(error.contains("asset partition 0 prefetch_budget 4 is below required budget 5"));
    }

    #[test]
    fn validate_asset_pack_rejects_missing_conditioning_evidence() {
        let mut manifest = valid_pack();
        manifest.chunks[0].conditioning_evidence.steps.clear();

        let error = validate_asset_pack(&manifest)
            .expect_err("expected missing conditioning evidence to fail closed");

        assert!(error.contains("missing required conditioning_evidence"));
    }

    #[test]
    fn validate_world_manifest_rejects_missing_asset_chunk_reference() {
        let asset_pack = valid_pack();
        let world = WorldChunkManifestV2 {
            schema_version: 3,
            kind: "world_chunk_manifest_v3".to_string(),
            world_id: "world-1".to_string(),
            partitions: vec![WorldChunkPartition {
                world_chunk_id: "world.chunk.0".to_string(),
                partition_id: 0,
            }],
            chunks: vec![WorldChunk {
                id: "world.chunk.0".to_string(),
                asset_chunk_ids: vec!["chunk.a".to_string(), "missing.asset".to_string()],
                hlod_asset_chunk_ids: vec!["chunk.a".to_string()],
                prefetch_neighbors: Vec::new(),
                refinement_sequence: vec![
                    WorldChunkRefinementStep {
                        stage: ConvergenceStage::Bootstrap,
                        asset_chunk_ids: vec!["chunk.a".to_string()],
                        hlod_asset_chunk_ids: vec!["chunk.a".to_string()],
                    },
                    WorldChunkRefinementStep {
                        stage: ConvergenceStage::Converged,
                        asset_chunk_ids: vec!["chunk.a".to_string(), "missing.asset".to_string()],
                        hlod_asset_chunk_ids: Vec::new(),
                    },
                ],
            }],
        };

        let error = validate_world_manifest(&asset_pack, &world)
            .expect_err("expected missing world asset reference error");

        assert!(error.contains(
            "missing asset chunk id 'missing.asset' referenced by world chunk 'world.chunk.0'"
        ));
    }

    #[test]
    fn validate_world_manifest_rejects_asymmetric_neighbors() {
        let asset_pack = valid_pack();
        let mut world = valid_world();
        world.chunks[1].prefetch_neighbors.clear();

        let error =
            validate_world_manifest(&asset_pack, &world).expect_err("expected neighbor mismatch");

        assert!(error.contains(
            "prefetch neighbor relationship must be symmetric between 'world.chunk.0' and 'world.chunk.1'"
        ));
    }

    #[test]
    fn validate_world_manifest_rejects_partition_cross_reference() {
        let asset_pack = valid_pack();
        let mut world = valid_world();
        world.chunks[0].asset_chunk_ids = vec!["chunk.c".to_string()];
        world.chunks[0].hlod_asset_chunk_ids = Vec::new();
        world.chunks[0].refinement_sequence = vec![
            WorldChunkRefinementStep {
                stage: ConvergenceStage::Bootstrap,
                asset_chunk_ids: vec!["chunk.c".to_string()],
                hlod_asset_chunk_ids: Vec::new(),
            },
            WorldChunkRefinementStep {
                stage: ConvergenceStage::Converged,
                asset_chunk_ids: vec!["chunk.c".to_string()],
                hlod_asset_chunk_ids: Vec::new(),
            },
        ];

        let error = validate_world_manifest(&asset_pack, &world)
            .expect_err("expected partition consistency failure");

        assert!(error.contains(
            "world chunk 'world.chunk.0' in partition 0 references chunk 'chunk.c' from partition 1"
        ));
    }

    #[test]
    fn validate_world_manifest_rejects_non_monotonic_refinement_stage_order() {
        let asset_pack = valid_pack();
        let mut world = valid_world();
        world.chunks[0].refinement_sequence = vec![
            WorldChunkRefinementStep {
                stage: ConvergenceStage::Bootstrap,
                asset_chunk_ids: vec!["chunk.a".to_string()],
                hlod_asset_chunk_ids: vec!["chunk.a".to_string()],
            },
            WorldChunkRefinementStep {
                stage: ConvergenceStage::Converged,
                asset_chunk_ids: vec!["chunk.a".to_string(), "chunk.b".to_string()],
                hlod_asset_chunk_ids: Vec::new(),
            },
            WorldChunkRefinementStep {
                stage: ConvergenceStage::Refine,
                asset_chunk_ids: vec!["chunk.a".to_string(), "chunk.b".to_string()],
                hlod_asset_chunk_ids: Vec::new(),
            },
        ];

        let error =
            validate_world_manifest(&asset_pack, &world).expect_err("expected stage-order error");

        assert!(error.contains("refinement_sequence must use strictly increasing stage order"));
    }

    #[test]
    fn validate_world_manifest_rejects_refinement_without_final_asset_coverage() {
        let asset_pack = valid_pack();
        let mut world = valid_world();
        world.chunks[0].refinement_sequence = vec![
            WorldChunkRefinementStep {
                stage: ConvergenceStage::Bootstrap,
                asset_chunk_ids: vec!["chunk.a".to_string()],
                hlod_asset_chunk_ids: vec!["chunk.a".to_string()],
            },
            WorldChunkRefinementStep {
                stage: ConvergenceStage::Converged,
                asset_chunk_ids: vec!["chunk.a".to_string()],
                hlod_asset_chunk_ids: Vec::new(),
            },
        ];

        let error = validate_world_manifest(&asset_pack, &world)
            .expect_err("expected final refinement coverage failure");

        assert!(
            error.contains("refinement_sequence converged stage must cover every asset_chunk_id")
        );
    }

    #[test]
    fn build_pack_index_is_deterministic() {
        let manifest = valid_pack();

        let first = build_pack_index(&manifest).expect("first index should build");
        let second = build_pack_index(&manifest).expect("second index should build");

        assert_eq!(first, second);

        let entries: Vec<(String, usize)> = first.into_iter().collect();
        assert_eq!(
            entries,
            vec![
                ("chunk.a".to_string(), 0),
                ("chunk.b".to_string(), 1),
                ("chunk.c".to_string(), 2),
            ]
        );
    }

    #[test]
    fn build_pack_index_rejects_duplicate_chunk_ids() {
        let manifest = pack(vec![
            chunk("chunk.a", 20, "high", &[]),
            chunk("chunk.a", 18, "normal", &[]),
            chunk("chunk.c", 12, "low", &[]),
        ]);

        let error = build_pack_index(&manifest).expect_err("duplicate ids should fail indexing");
        assert!(error.contains("duplicate asset chunk id: chunk.a"));
    }
}

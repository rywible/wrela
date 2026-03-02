#![cfg_attr(not(test), allow(dead_code))]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use wrla_asset_pack::{
    AssetPackManifestV3, WorldChunkManifestV2, validate_asset_pack, validate_world_manifest,
};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AudioAssetManifestEntry {
    pub id: String,
    pub path: String,
    pub kind: String,
    pub default_volume: f32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AssetManifestLoadSummary {
    pub(crate) loaded_chunk_count: u64,
    pub(crate) world_chunk_count: u64,
    pub(crate) loaded_bytes: u64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AssetFactoryManifestLoadSummary {
    pub(crate) generated_asset_count: u64,
    pub(crate) ui_atlas_count: u64,
    pub(crate) character_bundle_count: u64,
    pub(crate) provenance_entry_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AnimationEventWindowContract {
    pub(crate) id: String,
    pub(crate) start_frame: u32,
    pub(crate) end_frame: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AnimationStateContract {
    pub(crate) id: String,
    pub(crate) markers: Vec<String>,
    pub(crate) windows: Vec<AnimationEventWindowContract>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AnimationTransitionContract {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) after_ticks: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AnimationContractLoadSummary {
    pub(crate) graph_ref: String,
    pub(crate) clip_replay_hash: String,
    pub(crate) generated_clip_count: u64,
    pub(crate) quality_event_window_alignment: f64,
    pub(crate) states: Vec<AnimationStateContract>,
    pub(crate) transitions: Vec<AnimationTransitionContract>,
}

pub(crate) fn parse_and_validate_asset_pack_manifests_from_json(
    asset_pack_manifest_text: &str,
    world_chunk_manifest_text: &str,
    asset_pack_manifest_url: &str,
    world_chunk_manifest_url: &str,
) -> Result<AssetManifestLoadSummary, String> {
    let asset_pack_manifest =
        parse_asset_pack_manifest_json(asset_pack_manifest_text, asset_pack_manifest_url)?;
    let world_chunk_manifest =
        parse_world_chunk_manifest_json(world_chunk_manifest_text, world_chunk_manifest_url)?;

    validate_asset_world_manifest_contracts(
        &asset_pack_manifest,
        &world_chunk_manifest,
        asset_pack_manifest_url,
        world_chunk_manifest_url,
    )
}

fn parse_asset_pack_manifest_json(
    asset_pack_manifest_text: &str,
    asset_pack_manifest_url: &str,
) -> Result<AssetPackManifestV3, String> {
    let asset_pack_manifest: AssetPackManifestV3 = serde_json::from_str(asset_pack_manifest_text)
        .map_err(|error| {
        format!(
            "invalid asset pack manifest JSON at '{}': {error}",
            asset_pack_manifest_url
        )
    })?;
    if asset_pack_manifest.schema_version != 4 {
        return Err(format!(
            "asset pack manifest at '{}' must use schema_version=4 but found {}",
            asset_pack_manifest_url, asset_pack_manifest.schema_version
        ));
    }
    if asset_pack_manifest.kind != "asset_pack_manifest_v4" {
        return Err(format!(
            "asset pack manifest at '{}' has unexpected kind '{}' (expected 'asset_pack_manifest_v4')",
            asset_pack_manifest_url, asset_pack_manifest.kind
        ));
    }
    Ok(asset_pack_manifest)
}

fn parse_world_chunk_manifest_json(
    world_chunk_manifest_text: &str,
    world_chunk_manifest_url: &str,
) -> Result<WorldChunkManifestV2, String> {
    let world_chunk_manifest: WorldChunkManifestV2 =
        serde_json::from_str(world_chunk_manifest_text).map_err(|error| {
            format!(
                "invalid world chunk manifest JSON at '{}': {error}",
                world_chunk_manifest_url
            )
        })?;
    if world_chunk_manifest.schema_version != 3 {
        return Err(format!(
            "world chunk manifest at '{}' must use schema_version=3 but found {}",
            world_chunk_manifest_url, world_chunk_manifest.schema_version
        ));
    }
    if world_chunk_manifest.kind != "world_chunk_manifest_v3" {
        return Err(format!(
            "world chunk manifest at '{}' has unexpected kind '{}' (expected 'world_chunk_manifest_v3')",
            world_chunk_manifest_url, world_chunk_manifest.kind
        ));
    }
    Ok(world_chunk_manifest)
}

fn validate_asset_world_manifest_contracts(
    asset_pack_manifest: &AssetPackManifestV3,
    world_chunk_manifest: &WorldChunkManifestV2,
    asset_pack_manifest_url: &str,
    world_chunk_manifest_url: &str,
) -> Result<AssetManifestLoadSummary, String> {
    validate_asset_pack(asset_pack_manifest).map_err(|error| {
        format!(
            "asset pack manifest validation failed at '{}': {error}",
            asset_pack_manifest_url
        )
    })?;
    validate_world_manifest(asset_pack_manifest, world_chunk_manifest).map_err(|error| {
        format!(
            "world chunk manifest validation failed at '{}' against '{}': {error}",
            world_chunk_manifest_url, asset_pack_manifest_url
        )
    })?;

    let loaded_chunk_count = asset_pack_manifest.chunks.len() as u64;
    let loaded_bytes = asset_pack_manifest
        .chunks
        .iter()
        .fold(0u64, |total, chunk| total.saturating_add(chunk.bytes));
    let world_chunk_count = world_chunk_manifest.chunks.len() as u64;

    Ok(AssetManifestLoadSummary {
        loaded_chunk_count,
        world_chunk_count,
        loaded_bytes,
    })
}

pub(crate) fn parse_and_validate_asset_factory_manifests_from_json(
    factory_manifest_text: &str,
    provenance_manifest_text: &str,
    quality_manifest_text: &str,
    ui_manifest_text: &str,
    character_manifest_text: &str,
    factory_manifest_url: &str,
    provenance_manifest_url: &str,
    quality_manifest_url: &str,
    ui_manifest_url: &str,
    character_manifest_url: &str,
) -> Result<AssetFactoryManifestLoadSummary, String> {
    let factory_json = parse_json_manifest(
        factory_manifest_text,
        "asset factory manifest",
        factory_manifest_url,
    )?;
    let provenance_json = parse_json_manifest(
        provenance_manifest_text,
        "asset provenance ledger",
        provenance_manifest_url,
    )?;
    let quality_json = parse_json_manifest(
        quality_manifest_text,
        "asset quality report",
        quality_manifest_url,
    )?;
    let ui_json = parse_json_manifest(ui_manifest_text, "ui atlas manifest", ui_manifest_url)?;
    let character_json = parse_json_manifest(
        character_manifest_text,
        "character bundle manifest",
        character_manifest_url,
    )?;

    ensure_manifest_kind_version(
        &factory_json,
        2,
        "asset-factory-manifest-v2",
        "asset factory manifest",
        factory_manifest_url,
    )?;
    ensure_manifest_kind_version(
        &provenance_json,
        1,
        "asset-provenance-ledger-v1",
        "asset provenance ledger",
        provenance_manifest_url,
    )?;
    ensure_manifest_kind_version(
        &quality_json,
        2,
        "asset-quality-report-v2",
        "asset quality report",
        quality_manifest_url,
    )?;
    ensure_manifest_kind_version(
        &ui_json,
        1,
        "ui-atlas-manifest-v1",
        "ui atlas manifest",
        ui_manifest_url,
    )?;
    if let Some(kind) = character_json.get("kind").and_then(|value| value.as_str()) {
        if kind == "character-bundle-manifest-v1" || kind == "character-bundle-manifest-v2" {
            return Err(format!(
                "character bundle manifest at '{}' uses deprecated kind '{}'; regenerate artifacts with `character-bundle-manifest-v3` via `wrela game anim synth <path>` or `wrela game build <path>`",
                character_manifest_url, kind
            ));
        }
    }
    ensure_manifest_kind_version(
        &character_json,
        3,
        "character-bundle-manifest-v3",
        "character bundle manifest",
        character_manifest_url,
    )?;

    let generated_asset_count = factory_json
        .get("generated_assets")
        .and_then(|value| value.as_array())
        .map(|entries| entries.len() as u64)
        .unwrap_or(0);
    if generated_asset_count == 0 {
        return Err(format!(
            "asset factory manifest at '{}' must include non-empty generated_assets",
            factory_manifest_url
        ));
    }
    let generated_assets = factory_json
        .get("generated_assets")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    if generated_assets
        .iter()
        .any(|asset| !asset_has_conditioning_metadata(asset))
    {
        return Err(format!(
            "asset factory manifest at '{}' must include conditioning evidence, compression metadata, lod lineage/bounds, and deterministic hashes for every generated asset",
            factory_manifest_url
        ));
    }

    let quality_passed = quality_json
        .get("passed")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    if !quality_passed {
        return Err(format!(
            "asset quality report at '{}' must have passed=true",
            quality_manifest_url
        ));
    }
    let quality_asset_reports = quality_json
        .get("asset_reports")
        .and_then(|value| value.as_array())
        .ok_or_else(|| {
            format!(
                "asset quality report at '{}' must include non-empty asset_reports",
                quality_manifest_url
            )
        })?;
    if quality_asset_reports.is_empty() || quality_asset_reports.len() != generated_assets.len() {
        return Err(format!(
            "asset quality report at '{}' asset_reports must align to generated_assets count",
            quality_manifest_url
        ));
    }
    if quality_asset_reports
        .iter()
        .any(|asset| !asset_report_has_conditioning_metadata(asset))
    {
        return Err(format!(
            "asset quality report at '{}' contains quality.missing_conditioning_evidence",
            quality_manifest_url
        ));
    }

    let ui_atlases = ui_json
        .get("atlases")
        .and_then(|value| value.as_array())
        .ok_or_else(|| {
            format!(
                "ui atlas manifest at '{}' must include atlases array",
                ui_manifest_url
            )
        })?;
    if ui_atlases.is_empty() {
        return Err(format!(
            "ui atlas manifest at '{}' must include at least one atlas",
            ui_manifest_url
        ));
    }

    let character_bundles = character_json
        .get("bundles")
        .and_then(|value| value.as_array())
        .ok_or_else(|| {
            format!(
                "character bundle manifest at '{}' must include bundles array",
                character_manifest_url
            )
        })?;
    if character_bundles.is_empty() {
        return Err(format!(
            "character bundle manifest at '{}' must include at least one bundle",
            character_manifest_url
        ));
    }
    if character_bundles.iter().any(|bundle| {
        bundle
            .get("rig_ref")
            .and_then(|value| value.as_str())
            .map_or(true, |value| value.trim().is_empty())
            || bundle
                .get("graph_ref")
                .and_then(|value| value.as_str())
                .map_or(true, |value| value.trim().is_empty())
            || bundle
                .get("clip_set_ref")
                .and_then(|value| value.as_str())
                .map_or(true, |value| value.trim().is_empty())
            || bundle
                .get("skinning_profile")
                .and_then(|value| value.get("max_joints"))
                .and_then(|value| value.as_u64())
                .map_or(true, |value| value == 0)
    }) {
        return Err(format!(
            "character bundle manifest at '{}' must include rig_ref, graph_ref, clip_set_ref, and skinning_profile.max_joints for every bundle",
            character_manifest_url
        ));
    }

    let provenance_entries = provenance_json
        .get("entries")
        .and_then(|value| value.as_array())
        .ok_or_else(|| {
            format!(
                "asset provenance ledger at '{}' must include entries array",
                provenance_manifest_url
            )
        })?;
    if provenance_entries.is_empty() {
        return Err(format!(
            "asset provenance ledger at '{}' must include at least one entry",
            provenance_manifest_url
        ));
    }

    for entry in provenance_entries {
        let source_lineage = entry
            .get("source_lineage")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        if source_lineage.is_empty() {
            return Err(format!(
                "asset provenance ledger at '{}' contains provenance.unknown_lineage",
                provenance_manifest_url
            ));
        }
        let license_class = entry
            .get("license_class")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        if license_class != "rights-cleared" {
            return Err(format!(
                "asset provenance ledger at '{}' contains provenance.blocked_license",
                provenance_manifest_url
            ));
        }
        let attested = entry
            .get("attested")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let attestation_ref = entry
            .get("attestation_ref")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        if !attested || attestation_ref.is_empty() {
            return Err(format!(
                "asset provenance ledger at '{}' contains provenance.missing_attestation",
                provenance_manifest_url
            ));
        }
    }

    Ok(AssetFactoryManifestLoadSummary {
        generated_asset_count,
        ui_atlas_count: ui_atlases.len() as u64,
        character_bundle_count: character_bundles.len() as u64,
        provenance_entry_count: provenance_entries.len() as u64,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn parse_and_validate_animation_manifests_from_json(
    rig_catalog_text: &str,
    clip_bundle_text: &str,
    graph_contract_text: &str,
    flora_contract_text: &str,
    animation_quality_text: &str,
    rig_catalog_url: &str,
    clip_bundle_url: &str,
    graph_contract_url: &str,
    flora_contract_url: &str,
    animation_quality_url: &str,
) -> Result<AnimationContractLoadSummary, String> {
    let rig_catalog_json =
        parse_json_manifest(rig_catalog_text, "animation rig catalog", rig_catalog_url)?;
    let clip_bundle_json =
        parse_json_manifest(clip_bundle_text, "animation clip bundle", clip_bundle_url)?;
    let graph_contract_json = parse_json_manifest(
        graph_contract_text,
        "animation graph contract",
        graph_contract_url,
    )?;
    let flora_contract_json = parse_json_manifest(
        flora_contract_text,
        "flora sim contract",
        flora_contract_url,
    )?;
    let animation_quality_json = parse_json_manifest(
        animation_quality_text,
        "animation quality report",
        animation_quality_url,
    )?;

    ensure_manifest_kind_version(
        &rig_catalog_json,
        1,
        "animation-rig-catalog-v1",
        "animation rig catalog",
        rig_catalog_url,
    )?;
    if clip_bundle_json.get("kind").and_then(|value| value.as_str())
        == Some("animation-clip-bundle-v1")
    {
        return Err(format!(
            "animation clip bundle at '{}' uses deprecated kind 'animation-clip-bundle-v1'; regenerate artifacts with `animation-clip-bundle-v2`",
            clip_bundle_url
        ));
    }
    if graph_contract_json.get("kind").and_then(|value| value.as_str())
        == Some("animation-graph-contract-v1")
    {
        return Err(format!(
            "animation graph contract at '{}' uses deprecated kind 'animation-graph-contract-v1'; regenerate artifacts with `animation-graph-contract-v2`",
            graph_contract_url
        ));
    }
    if animation_quality_json
        .get("kind")
        .and_then(|value| value.as_str())
        == Some("animation-quality-report-v1")
    {
        return Err(format!(
            "animation quality report at '{}' uses deprecated kind 'animation-quality-report-v1'; regenerate artifacts with `animation-quality-report-v2`",
            animation_quality_url
        ));
    }
    ensure_manifest_kind_version(
        &clip_bundle_json,
        2,
        "animation-clip-bundle-v2",
        "animation clip bundle",
        clip_bundle_url,
    )?;
    ensure_manifest_kind_version(
        &graph_contract_json,
        2,
        "animation-graph-contract-v2",
        "animation graph contract",
        graph_contract_url,
    )?;
    ensure_manifest_kind_version(
        &flora_contract_json,
        1,
        "flora-sim-contract-v1",
        "flora sim contract",
        flora_contract_url,
    )?;
    ensure_manifest_kind_version(
        &animation_quality_json,
        2,
        "animation-quality-report-v2",
        "animation quality report",
        animation_quality_url,
    )?;

    let rigs = rig_catalog_json
        .get("rigs")
        .and_then(|value| value.as_array())
        .ok_or_else(|| {
            format!(
                "animation rig catalog at '{}' must include rigs array",
                rig_catalog_url
            )
        })?;
    if rigs.is_empty() {
        return Err(format!(
            "animation rig catalog at '{}' must include at least one rig",
            rig_catalog_url
        ));
    }

    let clip_sets = clip_bundle_json
        .get("clip_sets")
        .and_then(|value| value.as_array())
        .ok_or_else(|| {
            format!(
                "animation clip bundle at '{}' must include clip_sets array",
                clip_bundle_url
            )
        })?;
    if clip_sets.is_empty() {
        return Err(format!(
            "animation clip bundle at '{}' must include clip_sets entries",
            clip_bundle_url
        ));
    }
    let clips = clip_bundle_json
        .get("clips")
        .and_then(|value| value.as_array())
        .ok_or_else(|| {
            format!(
                "animation clip bundle at '{}' must include clips array",
                clip_bundle_url
            )
        })?;
    if clips.is_empty() {
        return Err(format!(
            "animation clip bundle at '{}' must include clips entries",
            clip_bundle_url
        ));
    }
    let clip_source = clip_bundle_json
        .get("source")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    if clip_source != "internal-deterministic-v2" {
        return Err(format!(
            "animation clip bundle at '{}' must use source='internal-deterministic-v2'",
            clip_bundle_url
        ));
    }

    let graphs = graph_contract_json
        .get("graphs")
        .and_then(|value| value.as_array())
        .ok_or_else(|| {
            format!(
                "animation graph contract at '{}' must include graphs array",
                graph_contract_url
            )
        })?;
    if graphs.is_empty() {
        return Err(format!(
            "animation graph contract at '{}' must include at least one graph",
            graph_contract_url
        ));
    }
    let first_graph = graphs
        .first()
        .and_then(|value| value.as_object())
        .ok_or_else(|| {
            format!(
                "animation graph contract at '{}' must encode graph objects",
                graph_contract_url
            )
        })?;
    let graph_ref = required_non_empty_field(
        first_graph.get("graph_ref"),
        "graph_ref",
        "animation graph contract",
        graph_contract_url,
    )?;
    let graph_states = first_graph
        .get("states")
        .and_then(|value| value.as_array())
        .ok_or_else(|| {
            format!(
                "animation graph contract at '{}' must include states array in graphs[0]",
                graph_contract_url
            )
        })?;
    if graph_states.is_empty() {
        return Err(format!(
            "animation graph contract at '{}' must include at least one state in graphs[0]",
            graph_contract_url
        ));
    }
    let mut states = Vec::<AnimationStateContract>::with_capacity(graph_states.len());
    let mut state_lookup = HashMap::<String, usize>::new();
    for state in graph_states {
        let state_object = state.as_object().ok_or_else(|| {
            format!(
                "animation graph contract at '{}' must encode state objects in graphs[0].states",
                graph_contract_url
            )
        })?;
        let id = required_non_empty_field(
            state_object.get("id"),
            "id",
            "animation graph state",
            graph_contract_url,
        )?;
        if state_lookup.insert(id.clone(), states.len()).is_some() {
            return Err(format!(
                "animation graph contract at '{}' has duplicate state id '{}'",
                graph_contract_url, id
            ));
        }
        let markers = match state_object.get("markers").and_then(|value| value.as_array()) {
            Some(items) => {
                let mut parsed = Vec::with_capacity(items.len());
                for marker in items {
                    let value = marker.as_str().map(str::trim).unwrap_or_default();
                    if value.is_empty() {
                        return Err(format!(
                            "animation graph contract at '{}' must encode non-empty markers for state '{}'",
                            graph_contract_url, id
                        ));
                    }
                    parsed.push(value.to_string());
                }
                parsed
            }
            None => Vec::new(),
        };
        states.push(AnimationStateContract {
            id,
            markers,
            windows: Vec::new(),
        });
    }

    let graph_transitions = first_graph
        .get("transitions")
        .and_then(|value| value.as_array())
        .ok_or_else(|| {
            format!(
                "animation graph contract at '{}' must include transitions array in graphs[0]",
                graph_contract_url
            )
        })?;
    if graph_transitions.is_empty() {
        return Err(format!(
            "animation graph contract at '{}' must include at least one transition in graphs[0]",
            graph_contract_url
        ));
    }
    let mut transitions = Vec::<AnimationTransitionContract>::with_capacity(graph_transitions.len());
    for transition in graph_transitions {
        let transition_object = transition.as_object().ok_or_else(|| {
            format!(
                "animation graph contract at '{}' must encode transition objects in graphs[0].transitions",
                graph_contract_url
            )
        })?;
        let from = required_non_empty_field(
            transition_object.get("from"),
            "from",
            "animation graph transition",
            graph_contract_url,
        )?;
        let to = required_non_empty_field(
            transition_object.get("to"),
            "to",
            "animation graph transition",
            graph_contract_url,
        )?;
        if !state_lookup.contains_key(from.as_str()) {
            return Err(format!(
                "animation graph contract at '{}' transition references unknown source state '{}'",
                graph_contract_url, from
            ));
        }
        if !state_lookup.contains_key(to.as_str()) {
            return Err(format!(
                "animation graph contract at '{}' transition references unknown target state '{}'",
                graph_contract_url, to
            ));
        }
        let blend_ms = transition_object
            .get("blend_ms")
            .and_then(|value| value.as_u64())
            .unwrap_or(16);
        let after_ticks = ((blend_ms.saturating_add(15)) / 16).max(1) as u32;
        transitions.push(AnimationTransitionContract {
            from,
            to,
            after_ticks,
        });
    }
    let cancel_windows = first_graph
        .get("cancel_windows")
        .and_then(|value| value.as_array())
        .ok_or_else(|| {
            format!(
                "animation graph contract at '{}' must include cancel_windows array in graphs[0]",
                graph_contract_url
            )
        })?;
    for window in cancel_windows {
        let window_object = window.as_object().ok_or_else(|| {
            format!(
                "animation graph contract at '{}' must encode cancel window objects in graphs[0].cancel_windows",
                graph_contract_url
            )
        })?;
        let id = required_non_empty_field(
            window_object.get("id"),
            "id",
            "animation graph cancel window",
            graph_contract_url,
        )?;
        let state_id = required_non_empty_field(
            window_object.get("state"),
            "state",
            "animation graph cancel window",
            graph_contract_url,
        )?;
        let start_frame = window_object
            .get("start_frame")
            .and_then(|value| value.as_u64())
            .ok_or_else(|| {
                format!(
                    "animation graph contract at '{}' must encode start_frame for cancel window '{}'",
                    graph_contract_url, id
                )
            })? as u32;
        let end_frame = window_object
            .get("end_frame")
            .and_then(|value| value.as_u64())
            .ok_or_else(|| {
                format!(
                    "animation graph contract at '{}' must encode end_frame for cancel window '{}'",
                    graph_contract_url, id
                )
            })? as u32;
        if start_frame == 0 {
            return Err(format!(
                "animation graph contract at '{}' cancel window '{}' must start at frame >= 1",
                graph_contract_url, id
            ));
        }
        if end_frame < start_frame {
            return Err(format!(
                "animation graph contract at '{}' cancel window '{}' has end_frame before start_frame",
                graph_contract_url, id
            ));
        }
        let Some(state_index) = state_lookup.get(state_id.as_str()).copied() else {
            return Err(format!(
                "animation graph contract at '{}' cancel window '{}' references unknown state '{}'",
                graph_contract_url, id, state_id
            ));
        };
        states[state_index].windows.push(AnimationEventWindowContract {
            id,
            start_frame,
            end_frame,
        });
    }

    let flora_wind_bands = flora_contract_json
        .get("wind_bands")
        .and_then(|value| value.as_array())
        .ok_or_else(|| {
            format!(
                "flora sim contract at '{}' must include wind_bands array",
                flora_contract_url
            )
        })?;
    if flora_wind_bands.is_empty() {
        return Err(format!(
            "flora sim contract at '{}' must include non-empty wind_bands",
            flora_contract_url
        ));
    }
    if flora_contract_json
        .get("integrates_with_animation_graph")
        .and_then(|value| value.as_bool())
        != Some(true)
    {
        return Err(format!(
            "flora sim contract at '{}' must set integrates_with_animation_graph=true",
            flora_contract_url
        ));
    }

    let quality_passed = animation_quality_json
        .get("passed")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    if !quality_passed {
        return Err(format!(
            "animation quality report at '{}' must have passed=true",
            animation_quality_url
        ));
    }
    if animation_quality_json
        .get("internal_generation_only")
        .and_then(|value| value.as_bool())
        != Some(true)
    {
        return Err(format!(
            "animation quality report at '{}' must set internal_generation_only=true",
            animation_quality_url
        ));
    }
    if animation_quality_json
        .get("external_asset_references")
        .and_then(|value| value.as_u64())
        != Some(0)
    {
        return Err(format!(
            "animation quality report at '{}' must set external_asset_references=0",
            animation_quality_url
        ));
    }

    let quality_generated_clip_count = animation_quality_json
        .get("generated_clip_count")
        .and_then(|value| value.as_u64())
        .ok_or_else(|| {
            format!(
                "animation quality report at '{}' must include generated_clip_count",
                animation_quality_url
            )
        })?;
    if quality_generated_clip_count != clips.len() as u64 {
        return Err(format!(
            "animation quality report at '{}' generated_clip_count must match animation clip bundle clips count at '{}'",
            animation_quality_url, clip_bundle_url
        ));
    }

    let quality_event_window_alignment = animation_quality_json
        .get("metrics")
        .and_then(|value| value.get("event_window_alignment"))
        .and_then(|value| value.as_f64())
        .ok_or_else(|| {
            format!(
                "animation quality report at '{}' must include metrics.event_window_alignment",
                animation_quality_url
            )
        })?;
    if !quality_event_window_alignment.is_finite()
        || !(0.0..=1.0).contains(&quality_event_window_alignment)
    {
        return Err(format!(
            "animation quality report at '{}' metrics.event_window_alignment must be within [0.0, 1.0]",
            animation_quality_url
        ));
    }

    let clip_replay_hash = clip_bundle_json
        .get("replay_hash")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .trim()
        .to_string();
    if clip_replay_hash.is_empty() {
        return Err(format!(
            "animation clip bundle at '{}' must include non-empty replay_hash",
            clip_bundle_url
        ));
    }
    let quality_replay_hash = animation_quality_json
        .get("replay_hash")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .trim()
        .to_string();
    if quality_replay_hash.is_empty() {
        return Err(format!(
            "animation quality report at '{}' must include non-empty replay_hash",
            animation_quality_url
        ));
    }
    if clip_replay_hash != quality_replay_hash {
        return Err(format!(
            "animation clip bundle replay_hash at '{}' must match animation quality report replay_hash at '{}'",
            clip_bundle_url, animation_quality_url
        ));
    }

    Ok(AnimationContractLoadSummary {
        graph_ref,
        clip_replay_hash,
        generated_clip_count: clips.len() as u64,
        quality_event_window_alignment,
        states,
        transitions,
    })
}

fn parse_json_manifest(text: &str, label: &str, url: &str) -> Result<serde_json::Value, String> {
    serde_json::from_str::<serde_json::Value>(text)
        .map_err(|error| format!("invalid {label} JSON at '{url}': {error}"))
}

fn ensure_manifest_kind_version(
    json: &serde_json::Value,
    schema_version: u64,
    kind: &str,
    label: &str,
    url: &str,
) -> Result<(), String> {
    if json.get("schema_version").and_then(|value| value.as_u64()) != Some(schema_version) {
        return Err(format!(
            "{label} at '{url}' must use schema_version={schema_version}"
        ));
    }
    if json.get("kind").and_then(|value| value.as_str()) != Some(kind) {
        return Err(format!(
            "{label} at '{url}' has unexpected kind (expected '{kind}')"
        ));
    }
    Ok(())
}

fn required_non_empty_field(
    value: Option<&serde_json::Value>,
    field: &str,
    label: &str,
    url: &str,
) -> Result<String, String> {
    let parsed = value
        .and_then(|item| item.as_str())
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    if parsed.is_empty() {
        return Err(format!("{label} at '{url}' must include non-empty {field}"));
    }
    Ok(parsed)
}

fn asset_has_conditioning_metadata(asset: &serde_json::Value) -> bool {
    asset
        .get("deterministic_hash")
        .and_then(|value| value.as_str())
        .is_some_and(|value| !value.trim().is_empty())
        && asset
            .get("conditioning_evidence")
            .and_then(|value| value.get("pipeline"))
            .and_then(|value| value.as_str())
            .is_some_and(|value| value == "asset-conditioning-v2")
        && asset
            .get("conditioning_evidence")
            .and_then(|value| value.get("steps"))
            .and_then(|value| value.as_array())
            .is_some_and(|steps| !steps.is_empty())
        && asset
            .get("compression")
            .and_then(|value| value.get("codec"))
            .and_then(|value| value.as_str())
            .is_some_and(|value| !value.trim().is_empty())
        && asset
            .get("lod")
            .and_then(|value| value.get("bounds"))
            .and_then(|value| value.get("min"))
            .and_then(|value| value.as_array())
            .is_some_and(|items| items.len() == 3)
        && asset
            .get("lod")
            .and_then(|value| value.get("bounds"))
            .and_then(|value| value.get("max"))
            .and_then(|value| value.as_array())
            .is_some_and(|items| items.len() == 3)
}

fn asset_report_has_conditioning_metadata(asset: &serde_json::Value) -> bool {
    asset.get("passed").and_then(|value| value.as_bool()) == Some(true)
        && asset_has_conditioning_metadata(asset)
}

#[cfg(test)]
mod tests {
    use super::{
        parse_and_validate_animation_manifests_from_json,
        parse_and_validate_asset_factory_manifests_from_json,
        parse_and_validate_asset_pack_manifests_from_json, parse_asset_pack_manifest_json,
        parse_world_chunk_manifest_json, validate_asset_world_manifest_contracts,
    };

    const ASSET_MANIFEST_URL: &str = "dist/assets-manifest.json";
    const WORLD_MANIFEST_URL: &str = "dist/world-chunks.json";
    const FACTORY_MANIFEST_URL: &str = "dist/asset-factory-manifest-v2.json";
    const PROVENANCE_MANIFEST_URL: &str = "dist/asset-provenance-ledger-v1.json";
    const QUALITY_MANIFEST_URL: &str = "dist/asset-quality-report-v2.json";
    const UI_MANIFEST_URL: &str = "dist/ui-atlas-manifest-v1.json";
    const CHARACTER_MANIFEST_URL: &str = "dist/character-bundle-manifest-v3.json";
    const ANIMATION_RIG_CATALOG_URL: &str = "dist/animation-rig-catalog-v1.json";
    const ANIMATION_CLIP_BUNDLE_URL: &str = "dist/animation-clip-bundle-v2.json";
    const ANIMATION_GRAPH_CONTRACT_URL: &str = "dist/animation-graph-contract-v2.json";
    const FLORA_SIM_CONTRACT_URL: &str = "dist/flora-sim-contract-v1.json";
    const ANIMATION_QUALITY_REPORT_URL: &str = "dist/animation-quality-report-v2.json";

    fn valid_asset_manifest_json() -> &'static str {
        r#"{
            "schema_version": 4,
            "kind": "asset_pack_manifest_v4",
            "pack_id": "pack-main",
            "streaming_budget_bytes": 280,
            "partitions": [
                {
                    "id": 0,
                    "chunk_ids": ["chunk.a"],
                    "residency_budget_bytes": 100,
                    "prefetch_budget": 2
                },
                {
                    "id": 1,
                    "chunk_ids": ["chunk.b"],
                    "residency_budget_bytes": 180,
                    "prefetch_budget": 3
                }
            ],
            "chunks": [
                {
                    "id": "chunk.a",
                    "path": "assets/chunk-a.bin",
                    "bytes": 64,
                    "checksum": 12345,
                    "dependencies": [],
                    "residency_priority": "normal",
                    "residency_class": "warm",
                    "convergence_stage": "stream",
                    "deterministic_hash": "f001cafe01234567",
                    "conditioning_evidence": {
                        "pipeline": "asset-conditioning-v2",
                        "source_hash": "source-chunk-a",
                        "deterministic_hash": "deadbeef01234567",
                        "steps": ["compress", "hash", "normalize"]
                    },
                    "compression": {
                        "codec": "store",
                        "uncompressed_bytes": 64,
                        "compressed_bytes": 64,
                        "ratio_milli": 1000,
                        "block_bytes": 4
                    },
                    "tile": {
                        "tile_width": 1,
                        "tile_height": 1,
                        "tile_layers": 1,
                        "tile_rows": 1,
                        "tile_columns": 1,
                        "total_tiles": 1,
                        "tile_format": "r8unorm"
                    },
                    "lod": {
                        "source_asset_id": "chunk.a",
                        "source_hash": "lod-source-a",
                        "max_lod": 1,
                        "bounds": { "min": [0, 0, 0], "max": [0, 0, 0] }
                    }
                },
                {
                    "id": "chunk.b",
                    "path": "assets/chunk-b.bin",
                    "bytes": 128,
                    "checksum": 67890,
                    "dependencies": ["chunk.a"],
                    "residency_priority": "high",
                    "residency_class": "core",
                    "convergence_stage": "bootstrap",
                    "deterministic_hash": "f001cafe89abcdef",
                    "conditioning_evidence": {
                        "pipeline": "asset-conditioning-v2",
                        "source_hash": "source-chunk-b",
                        "deterministic_hash": "deadbeef89abcdef",
                        "steps": ["compress", "hash", "normalize"]
                    },
                    "compression": {
                        "codec": "store",
                        "uncompressed_bytes": 128,
                        "compressed_bytes": 128,
                        "ratio_milli": 1000,
                        "block_bytes": 4
                    },
                    "tile": {
                        "tile_width": 1,
                        "tile_height": 1,
                        "tile_layers": 1,
                        "tile_rows": 1,
                        "tile_columns": 1,
                        "total_tiles": 1,
                        "tile_format": "r8unorm"
                    },
                    "lod": {
                        "source_asset_id": "chunk.b",
                        "source_hash": "lod-source-b",
                        "max_lod": 1,
                        "bounds": { "min": [0, 0, 0], "max": [0, 0, 0] }
                    }
                }
            ]
        }"#
    }

    fn valid_world_manifest_json() -> &'static str {
        r#"{
            "schema_version": 3,
            "kind": "world_chunk_manifest_v3",
            "world_id": "world-main",
            "partitions": [
                {
                    "world_chunk_id": "chunk.0",
                    "partition_id": 0
                },
                {
                    "world_chunk_id": "chunk.1",
                    "partition_id": 1
                }
            ],
            "chunks": [
                {
                    "id": "chunk.0",
                    "asset_chunk_ids": ["chunk.a"],
                    "hlod_asset_chunk_ids": ["chunk.a"],
                    "prefetch_neighbors": ["chunk.1"],
                    "refinement_sequence": [
                        {
                            "stage": "bootstrap",
                            "asset_chunk_ids": ["chunk.a"],
                            "hlod_asset_chunk_ids": ["chunk.a"]
                        },
                        {
                            "stage": "converged",
                            "asset_chunk_ids": ["chunk.a"],
                            "hlod_asset_chunk_ids": []
                        }
                    ]
                },
                {
                    "id": "chunk.1",
                    "asset_chunk_ids": ["chunk.b"],
                    "hlod_asset_chunk_ids": ["chunk.b"],
                    "prefetch_neighbors": ["chunk.0"],
                    "refinement_sequence": [
                        {
                            "stage": "bootstrap",
                            "asset_chunk_ids": ["chunk.b"],
                            "hlod_asset_chunk_ids": ["chunk.b"]
                        },
                        {
                            "stage": "converged",
                            "asset_chunk_ids": ["chunk.b"],
                            "hlod_asset_chunk_ids": []
                        }
                    ]
                }
            ]
        }"#
    }

    fn valid_factory_manifest_json() -> &'static str {
        r#"{
            "schema_version": 2,
            "kind": "asset-factory-manifest-v2",
            "generated_assets": [
                {
                    "asset_id": "asset.tree.oak",
                    "artifact_id": "artifact-1",
                    "path": "generated/asset.tree.oak/artifact-1.texture",
                    "bytes_len": 1024,
                    "fingerprint": "abc123",
                    "kind": "asset",
                    "deterministic_hash": "abc123abc123abc1",
                    "compression": {
                        "codec": "zstd",
                        "uncompressed_bytes": 1024,
                        "compressed_bytes": 1024,
                        "ratio_milli": 1000
                    },
                    "lod": {
                        "source_asset_id": "asset.tree.oak",
                        "source_hash": "source-asset-tree-oak",
                        "max_lod": 3,
                        "bounds": { "min": [-1000, -1000, -1000], "max": [1000, 1000, 1000] }
                    },
                    "conditioning_evidence": {
                        "pipeline": "asset-conditioning-v2",
                        "source_hash": "source-asset-tree-oak",
                        "deterministic_hash": "def456def456def4",
                        "steps": ["compress", "hash", "normalize"]
                    }
                }
            ]
        }"#
    }

    fn valid_provenance_manifest_json() -> &'static str {
        r#"{
            "schema_version": 1,
            "kind": "asset-provenance-ledger-v1",
            "policy": "rights-cleared-only",
            "strict": true,
            "entries": [
                {
                    "asset_id": "asset.tree.oak",
                    "source_lineage": "adapter://abc123",
                    "license_class": "rights-cleared",
                    "attestation_ref": "attest-1",
                    "attested": true
                }
            ]
        }"#
    }

    fn valid_quality_manifest_json() -> &'static str {
        r#"{
            "schema_version": 2,
            "kind": "asset-quality-report-v2",
            "passed": true,
            "asset_reports": [
                {
                    "asset_id": "asset.tree.oak",
                    "artifact_id": "artifact-1",
                    "passed": true,
                    "deterministic_hash": "abc123abc123abc1",
                    "compression": {
                        "codec": "zstd",
                        "uncompressed_bytes": 1024,
                        "compressed_bytes": 1024,
                        "ratio_milli": 1000
                    },
                    "lod": {
                        "source_asset_id": "asset.tree.oak",
                        "source_hash": "source-asset-tree-oak",
                        "max_lod": 3,
                        "bounds": { "min": [-1000, -1000, -1000], "max": [1000, 1000, 1000] }
                    },
                    "conditioning_evidence": {
                        "pipeline": "asset-conditioning-v2",
                        "source_hash": "source-asset-tree-oak",
                        "deterministic_hash": "def456def456def4",
                        "steps": ["compress", "hash", "normalize"]
                    }
                }
            ],
            "gates": {
                "conditioning_evidence": true
            }
        }"#
    }

    fn valid_ui_manifest_json() -> &'static str {
        r#"{
            "schema_version": 1,
            "kind": "ui-atlas-manifest-v1",
            "atlases": [
                { "id": "ui-default", "width": 1024, "height": 1024, "format": "rgba8unorm" }
            ]
        }"#
    }

    fn valid_character_manifest_json() -> &'static str {
        r#"{
            "schema_version": 3,
            "kind": "character-bundle-manifest-v3",
            "bundles": [
                {
                    "id": "hero-default",
                    "entity_class": "traveller",
                    "rig_ref": "rig/default-humanoid",
                    "graph_ref": "graph/default-humanoid-v2",
                    "clip_set_ref": "animset/default-humanoid",
                    "skinning_profile": {
                        "max_joints": 128,
                        "weights_per_vertex": 4
                    }
                }
            ]
        }"#
    }

    fn valid_animation_rig_catalog_json() -> &'static str {
        r#"{
            "schema_version": 1,
            "kind": "animation-rig-catalog-v1",
            "rigs": [
                { "rig_ref": "rig/default-humanoid", "bone_count": 64, "retarget_profile": "humanoid-v2" }
            ]
        }"#
    }

    fn valid_animation_clip_bundle_json() -> &'static str {
        r#"{
            "schema_version": 2,
            "kind": "animation-clip-bundle-v2",
            "source": "internal-deterministic-v2",
            "replay_hash": "clip-hash-001",
            "clip_sets": [
                {
                    "clip_set_ref": "animset/default-humanoid",
                    "clip_ids": ["animset/default-humanoid.idle"]
                }
            ],
            "clips": [
                {
                    "clip_id": "animset/default-humanoid.idle",
                    "clip_set_ref": "animset/default-humanoid",
                    "deterministic_clip_hash": "pose-hash-001"
                }
            ]
        }"#
    }

    fn valid_animation_graph_contract_json() -> &'static str {
        r#"{
            "schema_version": 2,
            "kind": "animation-graph-contract-v2",
            "graphs": [
                {
                    "graph_ref": "graph/default-humanoid-v2",
                    "states": [
                        { "id": "idle", "markers": ["foot_l"] },
                        { "id": "locomotion", "markers": ["foot_r"] }
                    ],
                    "transitions": [
                        { "from": "idle", "to": "locomotion", "blend_ms": 120 },
                        { "from": "locomotion", "to": "idle", "blend_ms": 80 }
                    ],
                    "cancel_windows": [
                        {
                            "id": "light_chain_window",
                            "state": "locomotion",
                            "start_frame": 4,
                            "end_frame": 11
                        }
                    ]
                }
            ]
        }"#
    }

    fn valid_flora_sim_contract_json() -> &'static str {
        r#"{
            "schema_version": 1,
            "kind": "flora-sim-contract-v1",
            "wind_bands": [0.05, 0.15, 0.3],
            "integrates_with_animation_graph": true
        }"#
    }

    fn valid_animation_quality_report_json() -> &'static str {
        r#"{
            "schema_version": 2,
            "kind": "animation-quality-report-v2",
            "passed": true,
            "generated_clip_count": 1,
            "internal_generation_only": true,
            "external_asset_references": 0,
            "replay_hash": "clip-hash-001",
            "metrics": {
                "event_window_alignment": 0.98
            }
        }"#
    }

    #[test]
    fn rejects_character_bundle_v1_after_cutover() {
        let legacy_character_manifest = r#"{
            "schema_version": 1,
            "kind": "character-bundle-manifest-v1",
            "bundles": [
                { "id": "hero-default", "skeleton": "humanoid-v1", "clip_count": 6, "retarget_contract": "humanoid-rig-v1" }
            ]
        }"#;

        let error = parse_and_validate_asset_factory_manifests_from_json(
            valid_factory_manifest_json(),
            valid_provenance_manifest_json(),
            valid_quality_manifest_json(),
            valid_ui_manifest_json(),
            legacy_character_manifest,
            FACTORY_MANIFEST_URL,
            PROVENANCE_MANIFEST_URL,
            QUALITY_MANIFEST_URL,
            UI_MANIFEST_URL,
            CHARACTER_MANIFEST_URL,
        )
        .expect_err("character bundle manifest v1 must be rejected after cutover");

        assert!(error.contains("deprecated kind 'character-bundle-manifest-v1'"));
        assert!(error.contains("wrela game anim synth <path>"));
    }

    #[test]
    fn rejects_character_bundle_v2_after_cutover() {
        let legacy_character_manifest = r#"{
            "schema_version": 2,
            "kind": "character-bundle-manifest-v2",
            "bundles": [
                {
                    "id": "hero-default",
                    "rig_ref": "rig/default-humanoid",
                    "anim_set_ref": "animset/default-humanoid",
                    "class_mapping": { "default": "player" }
                }
            ]
        }"#;

        let error = parse_and_validate_asset_factory_manifests_from_json(
            valid_factory_manifest_json(),
            valid_provenance_manifest_json(),
            valid_quality_manifest_json(),
            valid_ui_manifest_json(),
            legacy_character_manifest,
            FACTORY_MANIFEST_URL,
            PROVENANCE_MANIFEST_URL,
            QUALITY_MANIFEST_URL,
            UI_MANIFEST_URL,
            CHARACTER_MANIFEST_URL,
        )
        .expect_err("character bundle manifest v2 must be rejected after cutover");

        assert!(error.contains("deprecated kind 'character-bundle-manifest-v2'"));
        assert!(error.contains("character-bundle-manifest-v3"));
    }

    #[test]
    fn parse_and_validate_animation_manifests_passes_for_valid_contract_set() {
        let summary = parse_and_validate_animation_manifests_from_json(
            valid_animation_rig_catalog_json(),
            valid_animation_clip_bundle_json(),
            valid_animation_graph_contract_json(),
            valid_flora_sim_contract_json(),
            valid_animation_quality_report_json(),
            ANIMATION_RIG_CATALOG_URL,
            ANIMATION_CLIP_BUNDLE_URL,
            ANIMATION_GRAPH_CONTRACT_URL,
            FLORA_SIM_CONTRACT_URL,
            ANIMATION_QUALITY_REPORT_URL,
        )
        .expect("valid animation manifests should pass contract validation");

        assert_eq!(summary.graph_ref, "graph/default-humanoid-v2");
        assert_eq!(summary.generated_clip_count, 1);
        assert_eq!(summary.states.len(), 2);
        assert_eq!(summary.transitions.len(), 2);
        assert_eq!(summary.quality_event_window_alignment, 0.98);
    }

    #[test]
    fn parse_and_validate_animation_manifests_rejects_external_clip_source() {
        let invalid_clip_bundle = r#"{
            "schema_version": 2,
            "kind": "animation-clip-bundle-v2",
            "source": "external-import-v2",
            "replay_hash": "clip-hash-001",
            "clip_sets": [
                {
                    "clip_set_ref": "animset/default-humanoid",
                    "clip_ids": ["animset/default-humanoid.idle"]
                }
            ],
            "clips": [
                {
                    "clip_id": "animset/default-humanoid.idle",
                    "clip_set_ref": "animset/default-humanoid",
                    "deterministic_clip_hash": "pose-hash-001"
                }
            ]
        }"#;
        let error = parse_and_validate_animation_manifests_from_json(
            valid_animation_rig_catalog_json(),
            invalid_clip_bundle,
            valid_animation_graph_contract_json(),
            valid_flora_sim_contract_json(),
            valid_animation_quality_report_json(),
            ANIMATION_RIG_CATALOG_URL,
            ANIMATION_CLIP_BUNDLE_URL,
            ANIMATION_GRAPH_CONTRACT_URL,
            FLORA_SIM_CONTRACT_URL,
            ANIMATION_QUALITY_REPORT_URL,
        )
        .expect_err("external clip source must be rejected");
        assert!(error.contains("source='internal-deterministic-v2'"));
    }

    #[test]
    fn parse_and_validate_animation_manifests_rejects_v1_clip_bundle_after_cutover() {
        let legacy_clip_bundle = r#"{
            "schema_version": 1,
            "kind": "animation-clip-bundle-v1",
            "source": "internal-deterministic-v1",
            "replay_hash": "legacy-hash",
            "generated_clips": [
                { "clip_id": "animset/default-humanoid.idle", "pose_hash": "legacy-pose-hash" }
            ]
        }"#;
        let error = parse_and_validate_animation_manifests_from_json(
            valid_animation_rig_catalog_json(),
            legacy_clip_bundle,
            valid_animation_graph_contract_json(),
            valid_flora_sim_contract_json(),
            valid_animation_quality_report_json(),
            ANIMATION_RIG_CATALOG_URL,
            ANIMATION_CLIP_BUNDLE_URL,
            ANIMATION_GRAPH_CONTRACT_URL,
            FLORA_SIM_CONTRACT_URL,
            ANIMATION_QUALITY_REPORT_URL,
        )
        .expect_err("legacy clip schema must be rejected");
        assert!(error.contains("deprecated kind 'animation-clip-bundle-v1'"));
    }

    #[test]
    fn parse_and_validate_animation_manifests_rejects_v1_graph_contract_after_cutover() {
        let legacy_graph_contract = r#"{
            "schema_version": 1,
            "kind": "animation-graph-contract-v1",
            "states": ["idle", "locomotion"],
            "transitions": [{ "from": "idle", "to": "locomotion", "condition": "speed > 0.1" }]
        }"#;
        let error = parse_and_validate_animation_manifests_from_json(
            valid_animation_rig_catalog_json(),
            valid_animation_clip_bundle_json(),
            legacy_graph_contract,
            valid_flora_sim_contract_json(),
            valid_animation_quality_report_json(),
            ANIMATION_RIG_CATALOG_URL,
            ANIMATION_CLIP_BUNDLE_URL,
            ANIMATION_GRAPH_CONTRACT_URL,
            FLORA_SIM_CONTRACT_URL,
            ANIMATION_QUALITY_REPORT_URL,
        )
        .expect_err("legacy graph schema must be rejected");
        assert!(error.contains("deprecated kind 'animation-graph-contract-v1'"));
    }

    #[test]
    fn parse_and_validate_animation_manifests_rejects_v1_quality_report_after_cutover() {
        let legacy_quality_report = r#"{
            "schema_version": 1,
            "kind": "animation-quality-report-v1",
            "passed": true,
            "internal_generation_only": true,
            "external_asset_references": 0,
            "replay_hash": "clip-hash-001"
        }"#;
        let error = parse_and_validate_animation_manifests_from_json(
            valid_animation_rig_catalog_json(),
            valid_animation_clip_bundle_json(),
            valid_animation_graph_contract_json(),
            valid_flora_sim_contract_json(),
            legacy_quality_report,
            ANIMATION_RIG_CATALOG_URL,
            ANIMATION_CLIP_BUNDLE_URL,
            ANIMATION_GRAPH_CONTRACT_URL,
            FLORA_SIM_CONTRACT_URL,
            ANIMATION_QUALITY_REPORT_URL,
        )
        .expect_err("legacy quality schema must be rejected");
        assert!(error.contains("deprecated kind 'animation-quality-report-v1'"));
    }

    #[test]
    fn parse_and_validate_asset_manifests_returns_expected_summary_for_valid_pair() {
        let summary = parse_and_validate_asset_pack_manifests_from_json(
            valid_asset_manifest_json(),
            valid_world_manifest_json(),
            ASSET_MANIFEST_URL,
            WORLD_MANIFEST_URL,
        )
        .expect("valid manifests should pass contract validation");

        assert_eq!(summary.loaded_chunk_count, 2);
        assert_eq!(summary.world_chunk_count, 2);
        assert_eq!(summary.loaded_bytes, 192);
    }

    #[test]
    fn parse_asset_manifest_rejects_schema_mismatch_with_actionable_message() {
        let invalid_asset_manifest = r#"{
            "schema_version": 2,
            "kind": "asset_pack_manifest_v4",
            "pack_id": "pack-main",
            "streaming_budget_bytes": 0,
            "partitions": [],
            "chunks": []
        }"#;

        let error = parse_asset_pack_manifest_json(invalid_asset_manifest, ASSET_MANIFEST_URL)
            .expect_err("schema mismatch should fail");

        assert!(error.contains(
            "asset pack manifest at 'dist/assets-manifest.json' must use schema_version=4 but found 2"
        ));
    }

    #[test]
    fn parse_asset_manifest_rejects_invalid_json_with_actionable_message() {
        let error = parse_asset_pack_manifest_json("{", ASSET_MANIFEST_URL)
            .expect_err("invalid JSON should fail parsing");

        assert!(
            error.contains("invalid asset pack manifest JSON at 'dist/assets-manifest.json':"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn parse_asset_manifest_rejects_kind_mismatch() {
        let invalid_asset_manifest = r#"{
            "schema_version": 4,
            "kind": "wrong_asset_kind",
            "pack_id": "pack-main",
            "streaming_budget_bytes": 0,
            "partitions": [],
            "chunks": []
        }"#;

        let error = parse_asset_pack_manifest_json(invalid_asset_manifest, ASSET_MANIFEST_URL)
            .expect_err("kind mismatch should fail");

        assert!(error.contains(
            "asset pack manifest at 'dist/assets-manifest.json' has unexpected kind 'wrong_asset_kind' (expected 'asset_pack_manifest_v4')"
        ));
    }

    #[test]
    fn parse_world_manifest_rejects_invalid_json_with_actionable_message() {
        let error = parse_world_chunk_manifest_json("{", WORLD_MANIFEST_URL)
            .expect_err("invalid JSON should fail parsing");

        assert!(
            error.contains("invalid world chunk manifest JSON at 'dist/world-chunks.json':"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn parse_world_manifest_rejects_kind_mismatch() {
        let invalid_world_manifest = r#"{
            "schema_version": 3,
            "kind": "wrong_world_kind",
            "world_id": "world-main",
            "partitions": [],
            "chunks": []
        }"#;

        let error = parse_world_chunk_manifest_json(invalid_world_manifest, WORLD_MANIFEST_URL)
            .expect_err("kind mismatch should fail");

        assert!(error.contains(
            "world chunk manifest at 'dist/world-chunks.json' has unexpected kind 'wrong_world_kind' (expected 'world_chunk_manifest_v3')"
        ));
    }

    #[test]
    fn parse_world_manifest_rejects_schema_mismatch_with_actionable_message() {
        let invalid_world_manifest = r#"{
            "schema_version": 1,
            "kind": "world_chunk_manifest_v3",
            "world_id": "world-main",
            "partitions": [],
            "chunks": []
        }"#;

        let error = parse_world_chunk_manifest_json(invalid_world_manifest, WORLD_MANIFEST_URL)
            .expect_err("schema mismatch should fail");

        assert!(error.contains(
            "world chunk manifest at 'dist/world-chunks.json' must use schema_version=3 but found 1"
        ));
    }

    #[test]
    fn parse_and_validate_asset_manifests_rejects_fail_closed_unknown_fields() {
        let structurally_invalid_asset_manifest = r#"{
            "schema_version": 4,
            "kind": "asset_pack_manifest_v4",
            "pack_id": "pack-main",
            "streaming_budget_bytes": 280,
            "partitions": [
                {
                    "id": 0,
                    "chunk_ids": ["chunk.a"],
                    "residency_budget_bytes": 100,
                    "prefetch_budget": 2
                },
                {
                    "id": 1,
                    "chunk_ids": ["chunk.b"],
                    "residency_budget_bytes": 180,
                    "prefetch_budget": 3
                }
            ],
            "chunks": [
                {
                    "id": "chunk.a",
                    "path": "assets/chunk-a.bin",
                    "bytes": 64,
                    "checksum": 12345,
                    "dependencies": [],
                    "residency_priority": "normal",
                    "unknown_field": true
                },
                {
                    "id": "chunk.b",
                    "path": "assets/chunk-b.bin",
                    "bytes": 128,
                    "checksum": 67890,
                    "dependencies": ["chunk.a"],
                    "residency_priority": "high"
                }
            ]
        }"#;

        let error = parse_and_validate_asset_pack_manifests_from_json(
            structurally_invalid_asset_manifest,
            valid_world_manifest_json(),
            ASSET_MANIFEST_URL,
            WORLD_MANIFEST_URL,
        )
        .expect_err("unknown fields should fail closed");

        assert!(
            error.contains("invalid asset pack manifest JSON at 'dist/assets-manifest.json':"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn validate_asset_world_manifest_rejects_world_reference_to_missing_asset_chunk() {
        let world_with_missing_asset_chunk = r#"{
            "schema_version": 3,
            "kind": "world_chunk_manifest_v3",
            "world_id": "world-main",
            "partitions": [
                {
                    "world_chunk_id": "chunk.0",
                    "partition_id": 0
                }
            ],
            "chunks": [
                {
                    "id": "chunk.0",
                    "asset_chunk_ids": ["chunk.a", "missing.asset.chunk"],
                    "hlod_asset_chunk_ids": ["chunk.a"],
                    "prefetch_neighbors": [],
                    "refinement_sequence": [
                        {
                            "stage": "bootstrap",
                            "asset_chunk_ids": ["chunk.a"],
                            "hlod_asset_chunk_ids": ["chunk.a"]
                        },
                        {
                            "stage": "converged",
                            "asset_chunk_ids": ["chunk.a", "missing.asset.chunk"],
                            "hlod_asset_chunk_ids": []
                        }
                    ]
                }
            ]
        }"#;
        let asset_manifest =
            parse_asset_pack_manifest_json(valid_asset_manifest_json(), ASSET_MANIFEST_URL)
                .expect("asset manifest should parse");
        let world_manifest =
            parse_world_chunk_manifest_json(world_with_missing_asset_chunk, WORLD_MANIFEST_URL)
                .expect("world manifest should parse");

        let error = validate_asset_world_manifest_contracts(
            &asset_manifest,
            &world_manifest,
            ASSET_MANIFEST_URL,
            WORLD_MANIFEST_URL,
        )
        .expect_err("missing asset chunk reference should fail via world validation");

        assert!(error.contains(
            "world chunk manifest validation failed at 'dist/world-chunks.json' against 'dist/assets-manifest.json': missing asset chunk id 'missing.asset.chunk' referenced by world chunk 'chunk.0'"
        ));
    }

    #[test]
    fn parse_and_validate_asset_manifests_rejects_invalid_refinement_stage_order_pre_boot() {
        let invalid_world_manifest = r#"{
            "schema_version": 3,
            "kind": "world_chunk_manifest_v3",
            "world_id": "world-main",
            "partitions": [
                {
                    "world_chunk_id": "chunk.0",
                    "partition_id": 0
                },
                {
                    "world_chunk_id": "chunk.1",
                    "partition_id": 1
                }
            ],
            "chunks": [
                {
                    "id": "chunk.0",
                    "asset_chunk_ids": ["chunk.a"],
                    "hlod_asset_chunk_ids": ["chunk.a"],
                    "prefetch_neighbors": ["chunk.1"],
                    "refinement_sequence": [
                        {
                            "stage": "bootstrap",
                            "asset_chunk_ids": ["chunk.a"],
                            "hlod_asset_chunk_ids": ["chunk.a"]
                        },
                        {
                            "stage": "converged",
                            "asset_chunk_ids": ["chunk.a"],
                            "hlod_asset_chunk_ids": []
                        }
                    ]
                },
                {
                    "id": "chunk.1",
                    "asset_chunk_ids": ["chunk.b"],
                    "hlod_asset_chunk_ids": ["chunk.b"],
                    "prefetch_neighbors": ["chunk.0"],
                    "refinement_sequence": [
                        {
                            "stage": "bootstrap",
                            "asset_chunk_ids": ["chunk.b"],
                            "hlod_asset_chunk_ids": ["chunk.b"]
                        },
                        {
                            "stage": "converged",
                            "asset_chunk_ids": ["chunk.b"],
                            "hlod_asset_chunk_ids": []
                        },
                        {
                            "stage": "refine",
                            "asset_chunk_ids": ["chunk.b"],
                            "hlod_asset_chunk_ids": []
                        }
                    ]
                }
            ]
        }"#;

        let error = parse_and_validate_asset_pack_manifests_from_json(
            valid_asset_manifest_json(),
            invalid_world_manifest,
            ASSET_MANIFEST_URL,
            WORLD_MANIFEST_URL,
        )
        .expect_err("invalid refinement graph must fail pre-boot");

        assert!(error.contains("refinement_sequence must use strictly increasing stage order"));
    }

    #[test]
    fn parse_and_validate_asset_factory_manifests_returns_expected_summary() {
        let summary = parse_and_validate_asset_factory_manifests_from_json(
            valid_factory_manifest_json(),
            valid_provenance_manifest_json(),
            valid_quality_manifest_json(),
            valid_ui_manifest_json(),
            valid_character_manifest_json(),
            FACTORY_MANIFEST_URL,
            PROVENANCE_MANIFEST_URL,
            QUALITY_MANIFEST_URL,
            UI_MANIFEST_URL,
            CHARACTER_MANIFEST_URL,
        )
        .expect("valid asset factory manifests should pass");

        assert_eq!(summary.generated_asset_count, 1);
        assert_eq!(summary.ui_atlas_count, 1);
        assert_eq!(summary.character_bundle_count, 1);
        assert_eq!(summary.provenance_entry_count, 1);
    }

    #[test]
    fn parse_and_validate_asset_factory_manifests_rejects_v1_factory_schema() {
        let v1_factory_manifest = r#"{
            "schema_version": 1,
            "kind": "asset-factory-manifest-v1",
            "generated_assets": [
                {
                    "asset_id": "asset.tree.oak",
                    "artifact_id": "artifact-1",
                    "path": "generated/asset.tree.oak/artifact-1.texture",
                    "bytes_len": 1024,
                    "fingerprint": "abc123"
                }
            ]
        }"#;

        let error = parse_and_validate_asset_factory_manifests_from_json(
            v1_factory_manifest,
            valid_provenance_manifest_json(),
            valid_quality_manifest_json(),
            valid_ui_manifest_json(),
            valid_character_manifest_json(),
            FACTORY_MANIFEST_URL,
            PROVENANCE_MANIFEST_URL,
            QUALITY_MANIFEST_URL,
            UI_MANIFEST_URL,
            CHARACTER_MANIFEST_URL,
        )
        .expect_err("v1 factory schema must be rejected");

        assert!(error.contains("must use schema_version=2"));
    }

    #[test]
    fn parse_and_validate_asset_factory_manifests_fail_closed_on_missing_conditioning_evidence() {
        let invalid_factory_manifest = r#"{
            "schema_version": 2,
            "kind": "asset-factory-manifest-v2",
            "generated_assets": [
                {
                    "asset_id": "asset.tree.oak",
                    "artifact_id": "artifact-1",
                    "path": "generated/asset.tree.oak/artifact-1.texture",
                    "bytes_len": 1024,
                    "fingerprint": "abc123",
                    "kind": "asset"
                }
            ]
        }"#;

        let error = parse_and_validate_asset_factory_manifests_from_json(
            invalid_factory_manifest,
            valid_provenance_manifest_json(),
            valid_quality_manifest_json(),
            valid_ui_manifest_json(),
            valid_character_manifest_json(),
            FACTORY_MANIFEST_URL,
            PROVENANCE_MANIFEST_URL,
            QUALITY_MANIFEST_URL,
            UI_MANIFEST_URL,
            CHARACTER_MANIFEST_URL,
        )
        .expect_err("missing conditioning evidence should fail closed");

        assert!(error.contains("conditioning evidence"));
    }

    #[test]
    fn parse_and_validate_asset_factory_manifests_fail_closed_on_unknown_lineage() {
        let invalid_provenance = r#"{
            "schema_version": 1,
            "kind": "asset-provenance-ledger-v1",
            "policy": "rights-cleared-only",
            "strict": true,
            "entries": [
                {
                    "asset_id": "asset.tree.oak",
                    "source_lineage": "",
                    "license_class": "rights-cleared",
                    "attestation_ref": "attest-1",
                    "attested": true
                }
            ]
        }"#;
        let error = parse_and_validate_asset_factory_manifests_from_json(
            valid_factory_manifest_json(),
            invalid_provenance,
            valid_quality_manifest_json(),
            valid_ui_manifest_json(),
            valid_character_manifest_json(),
            FACTORY_MANIFEST_URL,
            PROVENANCE_MANIFEST_URL,
            QUALITY_MANIFEST_URL,
            UI_MANIFEST_URL,
            CHARACTER_MANIFEST_URL,
        )
        .expect_err("unknown lineage should fail");
        assert!(error.contains("provenance.unknown_lineage"));
    }

    #[test]
    fn parse_and_validate_asset_factory_manifests_fail_closed_on_blocked_license() {
        let invalid_provenance = r#"{
            "schema_version": 1,
            "kind": "asset-provenance-ledger-v1",
            "policy": "rights-cleared-only",
            "strict": true,
            "entries": [
                {
                    "asset_id": "asset.tree.oak",
                    "source_lineage": "adapter://abc",
                    "license_class": "cc-by-nc",
                    "attestation_ref": "attest-1",
                    "attested": true
                }
            ]
        }"#;
        let error = parse_and_validate_asset_factory_manifests_from_json(
            valid_factory_manifest_json(),
            invalid_provenance,
            valid_quality_manifest_json(),
            valid_ui_manifest_json(),
            valid_character_manifest_json(),
            FACTORY_MANIFEST_URL,
            PROVENANCE_MANIFEST_URL,
            QUALITY_MANIFEST_URL,
            UI_MANIFEST_URL,
            CHARACTER_MANIFEST_URL,
        )
        .expect_err("blocked license should fail");
        assert!(error.contains("provenance.blocked_license"));
    }

    #[test]
    fn parse_and_validate_asset_factory_manifests_fail_closed_on_missing_attestation() {
        let invalid_provenance = r#"{
            "schema_version": 1,
            "kind": "asset-provenance-ledger-v1",
            "policy": "rights-cleared-only",
            "strict": true,
            "entries": [
                {
                    "asset_id": "asset.tree.oak",
                    "source_lineage": "adapter://abc",
                    "license_class": "rights-cleared",
                    "attested": false
                }
            ]
        }"#;
        let error = parse_and_validate_asset_factory_manifests_from_json(
            valid_factory_manifest_json(),
            invalid_provenance,
            valid_quality_manifest_json(),
            valid_ui_manifest_json(),
            valid_character_manifest_json(),
            FACTORY_MANIFEST_URL,
            PROVENANCE_MANIFEST_URL,
            QUALITY_MANIFEST_URL,
            UI_MANIFEST_URL,
            CHARACTER_MANIFEST_URL,
        )
        .expect_err("missing attestation should fail");
        assert!(error.contains("provenance.missing_attestation"));
    }
}

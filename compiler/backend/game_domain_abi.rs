use crate::mir::ir::MirModule;
use std::fs;
use std::path::Path;

pub const DOMAIN_ABI_VERSION: &str = "app-kernel-v1";
pub const DETERMINISM_PROFILE: &str = "fixed-step-int-v2";
pub const FIXED_SCALE: i32 = 1024;
pub const WORLD_WIDTH: f32 = 800.0;
pub const WORLD_HEIGHT: f32 = 600.0;
pub const PLAYER_SPEED: f32 = 5.25;
pub const COLLISION_RADIUS: f32 = 28.0;
pub const WORLD_WIDTH_FIXED: i32 = 800 * FIXED_SCALE;
pub const WORLD_HEIGHT_FIXED: i32 = 600 * FIXED_SCALE;
pub const PLAYER_SPEED_FIXED: i32 = 5376;
pub const COLLISION_RADIUS_FIXED: i32 = 28 * FIXED_SCALE;
pub const COLLISION_RADIUS_SQ_FIXED: i64 =
    (COLLISION_RADIUS_FIXED as i64) * (COLLISION_RADIUS_FIXED as i64);

const STATE_SIZE_BYTES: u32 = 24;
const COLLECTIBLE_X_TABLE_OFFSET: u32 = 256;
const COLLECTIBLE_Y_TABLE_OFFSET: u32 = 512;
const FNV1A64_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV1A64_PRIME: u64 = 0x0000_0100_0000_01b3;
const HASH_PRIME: u64 = 1_099_511_628_211;

#[derive(Debug, Clone)]
pub struct GameDomainAbiDescriptor {
    pub domain_source_hash: String,
    pub source_seed: u64,
    pub collectible_positions: Vec<(i32, i32)>,
}

impl GameDomainAbiDescriptor {
    pub fn collectible_count(&self) -> usize {
        self.collectible_positions.len()
    }
}

pub fn describe_from_source_graph(
    mir_module: &MirModule,
    domain_source_hash: &str,
) -> GameDomainAbiDescriptor {
    let source_seed = compute_source_seed(mir_module, domain_source_hash);
    let collectible_count = compute_collectible_count(mir_module, source_seed);
    let collectible_positions = generate_collectible_positions(source_seed, collectible_count);
    GameDomainAbiDescriptor {
        domain_source_hash: domain_source_hash.to_string(),
        source_seed,
        collectible_positions,
    }
}

pub fn write_domain_wasm_artifact(
    output_path: &Path,
    descriptor: &GameDomainAbiDescriptor,
) -> Result<(), String> {
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create wasm artifact directory {}: {error}",
                parent.display()
            )
        })?;
    }

    let module_wat = build_module_wat(descriptor);
    let module_bytes = wat::parse_str(module_wat)
        .map_err(|error| format!("failed to encode domain wasm module: {error}"))?;
    fs::write(output_path, module_bytes).map_err(|error| {
        format!(
            "failed to write wasm artifact {}: {error}",
            output_path.display()
        )
    })
}

fn compute_source_seed(mir_module: &MirModule, domain_source_hash: &str) -> u64 {
    let mut hash = FNV1A64_OFFSET_BASIS;
    hash = fnv1a64_mix(hash, domain_source_hash.as_bytes());

    let mut function_signatures = mir_module
        .functions
        .iter()
        .map(|func| format!("{}:{}:{}", func.name, func.params.len(), func.blocks.len()))
        .collect::<Vec<_>>();
    function_signatures.sort();
    for signature in function_signatures {
        hash = fnv1a64_mix(hash, signature.as_bytes());
    }

    let mut class_signatures = mir_module
        .classes
        .iter()
        .map(|class| {
            let mut method_names = class
                .methods
                .iter()
                .map(|method| method.name.as_str())
                .collect::<Vec<_>>();
            method_names.sort_unstable();
            format!(
                "{}:{}:{}",
                class.name,
                class.fields.len(),
                method_names.join("|")
            )
        })
        .collect::<Vec<_>>();
    class_signatures.sort();
    for signature in class_signatures {
        hash = fnv1a64_mix(hash, signature.as_bytes());
    }

    hash
}

fn fnv1a64_mix(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV1A64_PRIME);
    }
    hash
}

fn compute_collectible_count(mir_module: &MirModule, source_seed: u64) -> usize {
    let base = 8usize;
    let seed_delta = (source_seed as usize) % 9;
    let mir_delta = mir_module.functions.len() % 8;
    (base + seed_delta + mir_delta).clamp(8, 31)
}

fn generate_collectible_positions(source_seed: u64, count: usize) -> Vec<(i32, i32)> {
    let mut rng = source_seed ^ 0x9e37_79b9_7f4a_7c15;
    let mut positions = Vec::with_capacity(count);
    let min = 48 * FIXED_SCALE;
    let max_x_exclusive = WORLD_WIDTH_FIXED - (48 * FIXED_SCALE) + 1;
    let max_y_exclusive = WORLD_HEIGHT_FIXED - (48 * FIXED_SCALE) + 1;
    for _ in 0..count {
        rng = splitmix64(rng);
        let x = sample_range_fixed(rng, min, max_x_exclusive);
        rng = splitmix64(rng);
        let y = sample_range_fixed(rng, min, max_y_exclusive);
        positions.push((x, y));
    }
    positions
}

fn splitmix64(mut state: u64) -> u64 {
    state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

fn sample_range_fixed(bits: u64, min: i32, max_exclusive: i32) -> i32 {
    let span = (max_exclusive - min).max(1) as u64;
    min + (bits % span) as i32
}

fn build_module_wat(descriptor: &GameDomainAbiDescriptor) -> String {
    let collectible_count = descriptor.collectible_count();
    let source_seed = descriptor.source_seed as i64;
    let x_table = descriptor
        .collectible_positions
        .iter()
        .flat_map(|(x, _)| x.to_le_bytes())
        .collect::<Vec<_>>();
    let y_table = descriptor
        .collectible_positions
        .iter()
        .flat_map(|(_, y)| y.to_le_bytes())
        .collect::<Vec<_>>();

    format!(
        r#"(module
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 4096))

  (func (export "wr_game_abi_version") (result i32)
    i32.const 1)

  (func (export "wr_game_state_size") (result i32)
    i32.const {state_size})

  (func (export "wr_game_alloc") (param $size i32) (result i32)
    (local $aligned i32)
    (local $ptr i32)
    local.get $size
    i32.const 7
    i32.add
    i32.const -8
    i32.and
    local.set $aligned
    global.get $heap
    local.set $ptr
    local.get $ptr
    local.get $aligned
    i32.add
    global.set $heap
    local.get $ptr)

  (func (export "wr_game_free") (param $ptr i32) (param $size i32))

  (func (export "wr_game_state_init") (param $ptr i32)
    local.get $ptr
    i64.const 0
    i64.store

    local.get $ptr
    i32.const 8
    i32.add
    i32.const {world_half_width_fixed}
    i32.store

    local.get $ptr
    i32.const 12
    i32.add
    i32.const {world_half_height_fixed}
    i32.store

    local.get $ptr
    i32.const 16
    i32.add
    i32.const 0
    i32.store

    local.get $ptr
    i32.const 20
    i32.add
    i32.const 0
    i32.store)

  (func (export "wr_game_state_apply_input")
    (param $ptr i32)
    (param $axis_x f32)
    (param $axis_y f32)
    (param $dt_ms i32)
    (local $dt i32)
    (local $x i32)
    (local $y i32)
    (local $idx i32)
    (local $mask i32)
    (local $score i32)
    (local $dx i32)
    (local $dy i32)
    (local $axis_x_fixed i32)
    (local $axis_y_fixed i32)
    (local $delta_x i32)
    (local $delta_y i32)

    local.get $dt_ms
    i32.const 1
    i32.lt_s
    if
      i32.const 1
      local.set $dt
    else
      local.get $dt_ms
      local.set $dt
    end

    local.get $axis_x
    f32.const {fixed_scale_f32}
    f32.mul
    i32.trunc_sat_f32_s
    local.set $axis_x_fixed

    local.get $axis_y
    f32.const {fixed_scale_f32}
    f32.mul
    i32.trunc_sat_f32_s
    local.set $axis_y_fixed

    local.get $axis_x_fixed
    i64.extend_i32_s
    i64.const {player_speed_fixed}
    i64.mul
    local.get $dt
    i64.extend_i32_s
    i64.mul
    i64.const {fixed_dt_denominator}
    i64.div_s
    i32.wrap_i64
    local.set $delta_x

    local.get $axis_y_fixed
    i64.extend_i32_s
    i64.const {player_speed_fixed}
    i64.mul
    local.get $dt
    i64.extend_i32_s
    i64.mul
    i64.const {fixed_dt_denominator}
    i64.div_s
    i32.wrap_i64
    local.set $delta_y

    local.get $ptr
    i32.const 8
    i32.add
    i32.load
    local.get $delta_x
    i32.add
    local.set $x

    local.get $ptr
    i32.const 12
    i32.add
    i32.load
    local.get $delta_y
    i32.add
    local.set $y

    local.get $x
    i32.const 0
    i32.lt_s
    if
      i32.const 0
      local.set $x
    end
    local.get $x
    i32.const {world_width_fixed}
    i32.gt_s
    if
      i32.const {world_width_fixed}
      local.set $x
    end

    local.get $y
    i32.const 0
    i32.lt_s
    if
      i32.const 0
      local.set $y
    end
    local.get $y
    i32.const {world_height_fixed}
    i32.gt_s
    if
      i32.const {world_height_fixed}
      local.set $y
    end

    local.get $ptr
    i32.const 8
    i32.add
    local.get $x
    i32.store

    local.get $ptr
    i32.const 12
    i32.add
    local.get $y
    i32.store

    local.get $ptr
    local.get $ptr
    i64.load
    i64.const 1
    i64.add
    i64.store

    local.get $ptr
    i32.const 20
    i32.add
    i32.load
    local.set $mask

    local.get $ptr
    i32.const 16
    i32.add
    i32.load
    local.set $score

    i32.const 0
    local.set $idx

    block $done
      loop $collect
        local.get $idx
        i32.const {collectible_count}
        i32.ge_u
        br_if $done

        local.get $mask
        i32.const 1
        local.get $idx
        i32.shl
        i32.and
        if
        else
          local.get $x
          i32.const {x_table_offset}
          local.get $idx
          i32.const 4
          i32.mul
          i32.add
          i32.load
          i32.sub
          local.set $dx

          local.get $y
          i32.const {y_table_offset}
          local.get $idx
          i32.const 4
          i32.mul
          i32.add
          i32.load
          i32.sub
          local.set $dy

          local.get $dx
          i64.extend_i32_s
          local.get $dx
          i64.extend_i32_s
          i64.mul
          local.get $dy
          i64.extend_i32_s
          local.get $dy
          i64.extend_i32_s
          i64.mul
          i64.add
          i64.const {collision_radius_sq_fixed}
          i64.lt_s
          if
            local.get $mask
            i32.const 1
            local.get $idx
            i32.shl
            i32.or
            local.set $mask

            local.get $score
            i32.const 1
            i32.add
            local.set $score
          end
        end

        local.get $idx
        i32.const 1
        i32.add
        local.set $idx
        br $collect
      end
    end

    local.get $ptr
    i32.const 16
    i32.add
    local.get $score
    i32.store

    local.get $ptr
    i32.const 20
    i32.add
    local.get $mask
    i32.store)

  (func (export "wr_game_state_hash") (param $ptr i32) (result i64)
    (local $hash i64)

    i64.const {source_seed}
    local.set $hash

    local.get $hash
    local.get $ptr
    i64.load
    i64.xor
    i64.const {hash_prime}
    i64.mul
    local.set $hash

    local.get $hash
    local.get $ptr
    i32.const 8
    i32.add
    i32.load
    i64.extend_i32_u
    i64.xor
    i64.const {hash_prime}
    i64.mul
    local.set $hash

    local.get $hash
    local.get $ptr
    i32.const 12
    i32.add
    i32.load
    i64.extend_i32_u
    i64.xor
    i64.const {hash_prime}
    i64.mul
    local.set $hash

    local.get $hash
    local.get $ptr
    i32.const 16
    i32.add
    i32.load
    i64.extend_i32_u
    i64.xor
    i64.const {hash_prime}
    i64.mul
    local.set $hash

    local.get $hash
    local.get $ptr
    i32.const 20
    i32.add
    i32.load
    i64.extend_i32_u
    i64.xor
    i64.const {hash_prime}
    i64.mul
    local.set $hash

    local.get $hash)

  (func (export "wr_game_state_tick") (param $ptr i32) (result i64)
    local.get $ptr
    i64.load)

  (func (export "wr_game_state_score") (param $ptr i32) (result i32)
    local.get $ptr
    i32.const 16
    i32.add
    i32.load)

  (func (export "wr_game_state_player_x") (param $ptr i32) (result f32)
    local.get $ptr
    i32.const 8
    i32.add
    i32.load
    f32.convert_i32_s
    f32.const {fixed_scale_f32}
    f32.div)

  (func (export "wr_game_state_player_y") (param $ptr i32) (result f32)
    local.get $ptr
    i32.const 12
    i32.add
    i32.load
    f32.convert_i32_s
    f32.const {fixed_scale_f32}
    f32.div)

  (func (export "wr_game_state_collected_mask") (param $ptr i32) (result i32)
    local.get $ptr
    i32.const 20
    i32.add
    i32.load)

  (func (export "wr_game_state_write")
    (param $ptr i32)
    (param $tick i64)
    (param $player_x f32)
    (param $player_y f32)
    (param $score i32)
    (param $mask i32)
    (local $x i32)
    (local $y i32)
    local.get $ptr
    local.get $tick
    i64.store

    local.get $player_x
    f32.const {fixed_scale_f32}
    f32.mul
    i32.trunc_sat_f32_s
    local.set $x

    local.get $player_y
    f32.const {fixed_scale_f32}
    f32.mul
    i32.trunc_sat_f32_s
    local.set $y

    local.get $x
    i32.const 0
    i32.lt_s
    if
      i32.const 0
      local.set $x
    end
    local.get $x
    i32.const {world_width_fixed}
    i32.gt_s
    if
      i32.const {world_width_fixed}
      local.set $x
    end

    local.get $y
    i32.const 0
    i32.lt_s
    if
      i32.const 0
      local.set $y
    end
    local.get $y
    i32.const {world_height_fixed}
    i32.gt_s
    if
      i32.const {world_height_fixed}
      local.set $y
    end

    local.get $ptr
    i32.const 8
    i32.add
    local.get $x
    i32.store

    local.get $ptr
    i32.const 12
    i32.add
    local.get $y
    i32.store

    local.get $ptr
    i32.const 16
    i32.add
    local.get $score
    i32.store

    local.get $ptr
    i32.const 20
    i32.add
    local.get $mask
    i32.store)

  (func (export "wr_game_collectible_x") (param $idx i32) (result f32)
    local.get $idx
    i32.const {collectible_count}
    i32.ge_u
    if (result f32)
      f32.const -1024
    else
      i32.const {x_table_offset}
      local.get $idx
      i32.const 4
      i32.mul
      i32.add
      i32.load
      f32.convert_i32_s
      f32.const {fixed_scale_f32}
      f32.div
    end)

  (func (export "wr_game_collectible_y") (param $idx i32) (result f32)
    local.get $idx
    i32.const {collectible_count}
    i32.ge_u
    if (result f32)
      f32.const -1024
    else
      i32.const {y_table_offset}
      local.get $idx
      i32.const 4
      i32.mul
      i32.add
      i32.load
      f32.convert_i32_s
      f32.const {fixed_scale_f32}
      f32.div
    end)

  (data (i32.const {x_table_offset}) "{x_table_bytes}")
  (data (i32.const {y_table_offset}) "{y_table_bytes}")
)"#,
        state_size = STATE_SIZE_BYTES,
        world_half_width_fixed = WORLD_WIDTH_FIXED / 2,
        world_half_height_fixed = WORLD_HEIGHT_FIXED / 2,
        world_width_fixed = WORLD_WIDTH_FIXED,
        world_height_fixed = WORLD_HEIGHT_FIXED,
        player_speed_fixed = PLAYER_SPEED_FIXED,
        collision_radius_sq_fixed = COLLISION_RADIUS_SQ_FIXED,
        collectible_count = collectible_count,
        x_table_offset = COLLECTIBLE_X_TABLE_OFFSET,
        y_table_offset = COLLECTIBLE_Y_TABLE_OFFSET,
        source_seed = source_seed,
        hash_prime = HASH_PRIME,
        fixed_scale_f32 = FIXED_SCALE as f32,
        fixed_dt_denominator = i64::from(16 * FIXED_SCALE),
        x_table_bytes = wat_bytes_literal(x_table.as_slice()),
        y_table_bytes = wat_bytes_literal(y_table.as_slice()),
    )
}

fn wat_bytes_literal(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 3);
    for byte in bytes {
        out.push('\\');
        out.push(nibble_to_hex((byte >> 4) & 0xf));
        out.push(nibble_to_hex(byte & 0xf));
    }
    out
}

fn nibble_to_hex(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'a' + (value - 10)) as char,
        _ => '0',
    }
}

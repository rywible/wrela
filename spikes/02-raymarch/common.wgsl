// Shared by bin.wgsl and trace.wgsl: the frame uniform and the posed instances.

struct Frame {
  eye: vec4f,       // xyz: camera; w: a pixel's angular size (radians per pixel at distance 1)
  cf: vec4f,        // camera forward
  cr: vec4f,        // camera right × tan(half fov x)
  cu: vec4f,        // camera up × tan(half fov y)
  sun: vec4f,       // xyz: direction toward the sun
  lc: vec4f,        // light grid: xyz centre, w half-extent (metres)
  lr: vec4f,        // light grid: right axis
  lu: vec4f,        // light grid: up axis
  dims: vec4u,      // width, height, tiles across, instance count
  grid: vec4u,      // x: light grid cells per side
}

@group(0) @binding(0) var<uniform> F: Frame;
@group(0) @binding(1) var<storage, read> inst: array<vec4f>;

// Per-instance layout, in vec4s (pose.js writes it).
const STRIDE: u32 = 81u;
const I_MISC: u32 = 0u;     // blend k, fbm amplitude, fbm frequency, fbm octaves
const I_SEED: u32 = 1u;     // xyz noise offset, w dapple frequency
const I_BOUND: u32 = 2u;    // instance bound sphere (world)
const I_TINV: u32 = 3u;     // torso bone, world → rest: 3 rows
const I_HINV: u32 = 6u;     // head bone, world → rest: 3 rows
const I_TORSO: u32 = 9u;    // torso params, rest space (spike 01's part 0)
const I_HEAD: u32 = 13u;    // head params, rest space (spike 01's part 2)
const I_TRAD: u32 = 17u;    // torso ellipsoids' inflated radii (2)
const I_HRAD: u32 = 19u;    // head ellipsoid's inflated radii
const I_MUZ: u32 = 20u;     // muzzle bound capsule (world): a.xyz + radius, b.xyz
const I_TSPH: u32 = 22u;    // torso bound spheres (world, 2)
const I_HSPH: u32 = 24u;    // head ellipsoid bound sphere (world)
const I_CONES: u32 = 25u;   // 26 cones (world): a.xyz + r1, b.xyz + r2
const I_PLANES: u32 = 77u;  // 4 hoof sole planes (world)

// PartMask bits, in the smooth union's fold order:
//   0 torso | 1–4 neck | 5 head | 6–11 tail | 12–23 legs | 24–27 hooves
const NPARTS: u32 = 28u;

// Tile and light-cell lists: [count, (instance, mask) × MAXE].
const MAXE: u32 = 16u;
const BIN_WORDS: u32 = 33u;

const INF: f32 = 1e30;
const BIG: f32 = 1e6;
const SALT: f32 = 0.0;      // replaced per run when measuring cold pipeline creation

// Stats slots (only written by the STATS variant, except overflows).
const S_CREATURE_PX: u32 = 0u;
const S_TERRAIN_PX: u32 = 1u;
const S_SKY_PX: u32 = 2u;
const S_MARCHES: u32 = 3u;
const S_STEPS: u32 = 4u;
const S_MISSES: u32 = 5u;
const S_CAPS: u32 = 6u;
const S_SHADOW_PX: u32 = 7u;
const S_SHADOW_MARCHES: u32 = 8u;
const S_SHADOW_STEPS: u32 = 9u;
const S_TERRAIN_STEPS: u32 = 10u;
const S_SCREEN_OVERFLOW: u32 = 11u;
const S_LIGHT_OVERFLOW: u32 = 12u;
const S_CAND_OVERFLOW: u32 = 13u;
const S_PART_EVALS: u32 = 14u;
const S_DETAIL_EVALS: u32 = 15u;
const S_TILE_ENTRIES: u32 = 16u;
const S_RELAX_FAILS: u32 = 17u;
const S_HIT_STEPS: u32 = 18u;
const S_SHADOW_CAPS: u32 = 19u;

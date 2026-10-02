// Cooks the per-cell tree attributes the grid walk needs before a tree's shape into a table: existence,
// species, the ground height under the trunk, the horizontal bound, the height and the trunk's jitter.
// It evaluates exactly what tree_lite + tree_full do inline (MAP off), so the content is unchanged.
// Appended to common.wgsl + foliage.wgsl; runs at start-up and again when the occupancy changes.

@group(0) @binding(10) var<storage, read_write> tmap_w: array<vec4u>;

@compute @workgroup_size(8, 8)
fn build_map(@builtin(global_invocation_id) gid: vec3u) {
  let n = u32(2 * MAP_HALF);
  if (gid.x >= n || gid.y >= n) { return; }
  let ci = vec2i(gid.xy) - vec2i(MAP_HALF);
  for (var slot = 0u; slot < 2u; slot++) {
    let h = tree_hash(ci, slot);
    var l: TreeLite;
    l.pos = tree_pos(ci, h);
    l.exists = u01(h.x) < tree_occ(slot) * forest_mask(l.pos.x, l.pos.y);
    var e = vec4u(0u);
    if (l.exists) {
      let t = tree_full(l, ci, slot);
      e = vec4u(1u | (t.kind << 1u), bitcast<u32>(t.base), pack2x16float(vec2f(tree_bound_r(t), t.height + 0.5)),
                pack2x16unorm(vec2f(u01(h.y), u01(h.z))));
    }
    tmap_w[(gid.y * n + gid.x) * 2u + slot] = e;
  }
}

# Language items (deleted when the work is done)

The ask: every item of the language review, fully; the sky fixes; then milestone-5 fast-forwarded
to the result. Decided: pipelines that run only at load count against the 64.

## Sky branch fixes
- [x] A label names everything up to the next (both hosts); the slice test claims every label
- [x] One crown tag for cards, impostors and volumes
- [x] One mapping for each air table's cook and reader; a round-trip test
- [x] The impostor's volume in 3D textures

## Language
- [x] 1 Groups of GPU resources: a borrow struct as one parameter, in GPU code too; WebGPU's
      per-stage limits named by field; the engine's lighting inputs as one
- [x] 4 Texture formats as types: `Texture<F>`, writable by a type-bounded constructor, integer loads
- [x] 2 Dispatch over a domain: `over:`; the compiler sizes the groups and checks the bounds
      (a kernel with workgroup memory can't take `over:`)
- [x] 3 Kernel outputs: typed indirect arguments; a single-writer output (`One<T>`); a span of
      one field (`buf.at(i).field`). No `Texels` tile: nothing needs a window yet, and writes at
      computed coordinates break invocation safety
- [x] 5a Test-only entry points and exports out of a shipped build (`@testing`, `test_build()`;
      the clearing ships 72 pipelines, 75 in a test build)
- [ ] 6 Bound entry points as compile-time values. Design: each entry point has a hidden
      borrow struct of its arguments (its generics; buffers as spans, textures borrowed, values
      by value); `k.bind(...)` outside a command is a literal of it; `dispatch`/`draw` take a
      local of it, or a generic `F: Kernel | Vertex | Fragment` (lang traits it implements),
      resolved at lowering from the concrete type; args are its fields' places
- [ ] 5b Frame tests on the native host's GPU, in wrela; exports take fieldless enums
- [ ] 7 Work over several frames
- [ ] Small: `borrow x = if c { a } else { b }`; packed bitfields; enums to and from `u32`;
      flags; unused-item warning; GpuData padding; non-square matrices
- [ ] Budget: 64 pipelines per scene, checked; merged instantiations (opt-in); load cooks merged
- [ ] Docs: language.md, grammar; vision.md; #26
- [ ] `--long`; milestone-5 fast-forwarded

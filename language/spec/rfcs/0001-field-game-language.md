# RFC 0001: Field-Native Game Language For Wrela

Status: Proposed

Author: Codex

Created: 2026-04-02

Target: `wrela` language, compiler, runtime, and build pipeline

## Summary

This RFC defines a hard-cut evolution of `wrela` into a field-native game language suitable for shipping a real game such as `Staircase`.

The resulting language is not a traditional engine scripting layer and is not a general "shader language inside Wrela". It is a unified language for:

- field-authored world geometry
- procedural materials, radiance, and volumetric effects
- streamed world composition
- CPU and GPU dual-target emission from one portable source
- deterministic host-side gameplay systems
- physics-authored but predefined combat moves
- standalone native binary packaging

All authored game content is represented as fields, custom materials, world generators, and mutable gameplay state. Authored meshes, textures, prebaked distance fields, prebaked material maps, and prebaked animation clips are out of scope.

The compiler is responsible for lowering this source language into:

- CPU-native code for tests, gameplay queries, tools, and fallback execution
- GPU-native programs for presentation
- runtime metadata and derived execution artifacts needed for streaming and acceleration

## Motivation

The design target is a game like `Staircase`:

- an endless spiraling staircase
- first-person camera
- telekinetic katana combat
- movesets authored from physics instead of prebaked animation clips
- a finite sequence of authored boss encounters
- platforms/landings at authored junctures
- bosses that deform the terrain under the player
- impossible geometry and shapes that play to the strengths of field composition

Traditional game engines do not give the compiler enough semantic structure to deeply optimize such a world. They treat most authored content as opaque assets plus runtime scripts. This RFC instead makes the authored world algebraic and compiler-visible:

- geometry is field composition
- appearance is field-driven material and radiance
- effects are media fields
- world streaming is region composition
- dynamic entities are archetypes plus state
- traversal and rendering are query domains over captured world state

This makes it possible to:

- share logic between CPU and GPU
- test geometry, collision, visibility, and rendering behavior on CPU
- specialize queries by domain
- derive acceleration data from authored structure
- keep authored content purely field-based without falling back to imported assets

## Design Goals

1. Make `wrela` a coherent field-native language for real games, not a toy rendering DSL.
2. Author all game content as code plus parameters plus state, with no authored mesh/texture asset fallback.
3. Support a portable subset that compiles to both CPU and GPU.
4. Keep `system` as the host orchestration layer for mutation, gameplay logic, and runtime coordination.
5. Make streaming and chunking first-class language concerns rather than an afterthought.
6. Support materials, lighting, media, and effects as first-class authored constructs.
7. Support physics-authored but predefined combat moves as first-class authored constructs.
8. Produce standalone native binaries that bundle runtime support and embedded portable programs.
9. Preserve a clean separation between authored content and derived execution artifacts.
10. Make the language implementation-friendly: explicit types, explicit semantics, explicit lowering contracts.
11. Make authored code highly testable with CPU-reference execution and first-class GPU differential tooling.

## Non-Goals

1. Backward compatibility with existing or placeholder render syntax.
2. Imported meshes, textures, prebaked SDFs, or prebaked animation clips.
3. A traditional engine object model centered on scene graphs, prefabs, and opaque materials.
4. User-authored low-level march loops as the primary rendering abstraction.
5. Automatic exactness for all authored fields.
6. One universal evaluator that serves rendering, collision, navigation, and audio equally.
7. Mixed dynamic/host features inside portable code.

## Rationale And Rejected Alternatives

### Compile Arbitrary Wrela To GPU

This RFC rejects the idea of lowering arbitrary host-language `wrela` directly to GPU execution.

General host-language features such as heap identity, strings, maps, actor/message behavior, pending values, async effects, host IO, and dynamic dispatch do not map cleanly or safely to GPU execution. More importantly, they destroy the compiler's ability to reason about spatial semantics, exactness, purity, and evaluation cost.

The adopted model is a distinct portable lane with first-class spatial declarations and strict type/effect rules.

### One Global World Field

This RFC rejects authoring a game world as one giant `world(p)` field and then attempting to chunk or stream it afterward.

That model fails to encode:

- streamable region identity
- support metadata
- local frames
- domain-specialized overrides
- dynamic state-bound instance sets
- phase-driven world variants

Instead, the language authors the world as a composition graph of finite local-space regions from the beginning.

### Imported Asset Escape Hatches

This RFC rejects imported meshes, textures, prebaked signed-distance fields, and prebaked animation clips as authored content.

Allowing those escape hatches would collapse the language back into a conventional engine asset pipeline and deprive the compiler of the structural knowledge needed for aggressive analysis and dual-target code generation.

Derived execution artifacts remain allowed, but they are never source-of-truth content.

### User-Authored March Loops

This RFC rejects exposing hand-authored raymarch loops as the main rendering abstraction.

The source language describes fields, materials, media, and domain policies. The compiler chooses the appropriate evaluation strategy per domain and per region. That strategy MAY include analytic hits, interval refinement, bounded stepping, sphere tracing, or domain-specific hybrid execution.

### One Evaluator For Every System

This RFC rejects the notion that rendering, collision, visibility, navigation, and audio should all consume one universal world evaluator contract.

Those systems have different correctness, cost, and data requirements. The language instead requires first-class query domains with distinct detail levels, error budgets, and layer participation rules.

## Operational Considerations

### Determinism

Host-lane gameplay systems SHOULD remain deterministic where gameplay authority depends on them.

Portable presentation code is not required to be bitwise deterministic across CPU and GPU. Presentation equivalence is established through tolerances, image comparisons, and domain-specific correctness criteria.

### Save Stability

Because the world is authored as executable generators instead of imported assets, save stability becomes a language and runtime concern.

Implementations SHOULD version:

- region generators
- archetype contracts
- phase layouts
- payload schemas
- persistent handle formats

This RFC does not define a compatibility regime, but it explicitly acknowledges that generator changes can invalidate save expectations if no version discipline exists.

For persistent worlds and saved games, the implementation MUST preserve the following stability properties for unchanged generator code and unchanged explicit inputs:

- region-local generated content MUST be identical for identical region parameters
- scatter placement MUST be identical for identical seeds, masks, densities, and phase inputs
- persistent handles and payload identity derivation MUST NOT depend on residency order or runtime insertion order
- generation output MUST NOT depend on mutable global state or hidden ambient randomness

If a generator depends on neighboring regions, that dependency MUST be explicit and bounded so that the implementation can reason about save and streaming stability.

### Performance Envelope

A conforming implementation is expected to rely heavily on:

- support-based broadphase
- local-space evaluation
- domain-specific detail selection
- derived execution artifacts
- streaming residency control

An implementation that attempts to evaluate all fields at presentation detail for every domain is non-viable for the target class of games and does not satisfy the intent of this RFC.

### Specialization And Artifact Budgets

Derived execution artifacts and domain specializations are required by this RFC, but they MUST remain budgeted and observable.

Implementations MUST define bounded policies for:

- compile-time specialization fanout
- runtime specialization creation
- runtime eviction and reuse
- GPU pipeline creation and reuse
- CPU and GPU cache key stability

Specialization and artifact creation MUST be keyed by explicit inputs such as:

- target backend
- declaration bundle identity
- domain identity
- detail level
- phase-specialized variant identity
- relevant layout/version metadata

Specialization MUST NOT depend on transient residency order, frame timing, or unrelated runtime mutation.

## Terminology

- Host lane: The mutable, orchestration-oriented part of `wrela`. This includes `fn`, `system`, `resource`, IO, async, and runtime coordination.
- Portable lane: The pure, fixed-layout part of `wrela` that can be compiled to both CPU and GPU. This includes `value`, `kernel fn`, `field`, `material`, `radiance field`, `volume field`, and portable portions of `generator`, `archetype`, and `body`.
- Field: A spatially evaluated function with compiler-known semantics.
- Exact distance field: A field that satisfies the language's signed-distance contract under the exact operation subset.
- Conservative distance field: A field that returns a safe lower bound for traversal and pruning but may not be exact.
- Generator: A parameterized producer of field-authored content with no runtime identity.
- Archetype: A reusable compiled field family that can be instantiated from runtime state.
- Region: A streamed unit of world composition.
- Capture: An immutable query epoch over a composed world.
- Domain: A query policy describing how a world is evaluated for a specific purpose such as collision or presentation.
- Derived execution artifact: A compiler- or runtime-generated structure such as a support hierarchy, acceleration table, filtered noise cache, or residency map derived from source plus state. These are allowed and expected.

## Normative Keywords

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", and "MAY" in this document are to be interpreted as described in RFC 2119.

## Core Principles

### 1. Authored Content Is Field-Only

Authored game content MUST be represented as one or more of:

- fields
- generators
- archetypes
- bodies
- materials
- radiance fields
- volume fields
- host-owned runtime state

The following are forbidden as authored content:

- meshes
- texture maps
- prebaked material maps
- prebaked signed distance fields
- prebaked volumetric assets
- prebaked animation clips

### 2. Derived Execution Artifacts Are Allowed

The compiler and runtime MAY derive transient or persisted execution artifacts from authored code and state, including:

- support hierarchies
- interval bounds
- filtered noise tables
- query acceleration structures
- domain-specialized evaluator bundles
- render-specialized transient caches

These artifacts MUST be reproducible from source plus runtime state and MUST NOT be treated as authored content.

### 3. The World Is A Composition Graph Of Local Fields

The world MUST NOT be authored as a single giant global field. Instead, the world MUST be expressed as:

- finite local-space field regions
- composition rules between regions
- streamed region descriptor sets
- dynamic state-bound archetype instances

### 4. CPU And GPU Share One Portable Source

The portable lane MUST compile to:

- CPU-native code for tests, tools, and gameplay queries
- GPU-native code for presentation and GPU-side query execution when appropriate

CPU and GPU backends MUST consume the same portable IR, not two independent frontends.

### 5. Query Domains Are First-Class

Different gameplay and presentation systems MUST query different world domains. The language MUST support domain-specific evaluation contracts so that:

- presentation can use fine geometry, materials, radiance, and media
- collision can use coarse geometry only
- navigation can use coarse or specialized walkability fields
- visibility can use cheaper or differently filtered geometry

## Source Language Additions

### New Declarations

The language SHALL add the following declarations:

```wr
value Name { ... }
enum Name { ... }
phase Name { ... }
resource Name { ... }

kernel fn name(...) -> T { ... }

field exact distance name(...) -> F32 { ... }
field conservative distance name(...) -> F32 { ... }
material name(hit: Hit3, ...) -> Surface { ... }
radiance field name(...) -> Vec3 { ... }
volume field name(...) -> Medium { ... }

generator Name(params...) { ... }
archetype Name(instance: T) { ... }
body Name(instance: T) { ... }

move Name(...) { ... }
moveset Name { ... }

region Name(params...) { ... }
transition Name(left: A, right: B, ...) { ... }
space Name { ... }

domain Name(world: Capture[World], ...) { ... }
render Name(world: Capture[World], camera: Camera) { ... }

test name() { ... }
```

These declarations are described in the following sections.

### Grammar Additions

This RFC defines the following guide-level grammar additions. The final parser grammar MAY vary in exact token choices so long as the semantic model is preserved.

#### Declaration Grammar

```ebnf
top_level_decl
  = value_decl
  | enum_decl
  | phase_decl
  | resource_decl
  | kernel_decl
  | field_decl
  | material_decl
  | radiance_decl
  | volume_decl
  | generator_decl
  | archetype_decl
  | body_decl
  | move_decl
  | moveset_decl
  | region_decl
  | transition_decl
  | space_decl
  | domain_decl
  | render_decl
  | test_decl
  | host_fn_decl
  | system_decl

value_decl
  = "value" IDENT "{" value_field* "}"

phase_decl
  = "phase" IDENT "{" phase_case+ "}"

kernel_decl
  = "kernel" "fn" IDENT param_list "->" type block

field_decl
  = "field" field_class field_kind IDENT param_list "->" "F32" block

field_class
  = "exact" | "conservative"

field_kind
  = "distance"

material_decl
  = "material" IDENT param_list "->" "Surface" block

radiance_decl
  = "radiance" "field" IDENT param_list "->" "Vec3" block

volume_decl
  = "volume" "field" IDENT param_list "->" "Medium" block
```

#### Composite Spatial Declaration Grammar

```ebnf
generator_decl
  = "generator" IDENT param_list "{" generator_item* "}"

archetype_decl
  = "archetype" IDENT "(" "instance" ":" type ")" "{" archetype_item* "}"

body_decl
  = "body" IDENT "(" "instance" ":" type ")" "{" body_item* "}"

generator_item
  = support_item
  | detail_field_item
  | material_item
  | radiance_item
  | volume_item

archetype_item
  = transform_item
  | deform_item
  | support_item
  | detail_field_item
  | terrain_field_item
  | material_item
  | radiance_item
  | volume_item
  | payload_item

body_item
  = mass_item
  | inertia_item
  | deform_item
  | support_item
  | collision_field_item
  | material_item
  | payload_item

detail_field_item
  = "detail" IDENT "field" field_class field_kind param_list "->" "F32" block

terrain_field_item
  = "terrain" "detail" IDENT "field" field_class field_kind param_list "->" "F32" block

collision_field_item
  = "collision" "detail" IDENT "field" field_class field_kind param_list "->" "F32" block

support_item
  = ("support" | "coarse_support" | "tight_support") "=" expr

transform_item
  = "transform" "=" expr

deform_item
  = "deform" "=" expr

payload_item
  = "payload" "=" expr
```

#### Move Grammar

```ebnf
move_decl
  = "move" IDENT param_list "{" move_item* "}"

move_item
  = duration_item
  | move_phase_decl

duration_item
  = "duration" "=" expr

move_phase_decl
  = "phase" IDENT "[" expr ".." expr "]" "{" move_op* "}"
```

#### Region And World Grammar

```ebnf
region_decl
  = "region" IDENT param_list "{" region_stmt* "}"

transition_decl
  = "transition" IDENT param_list "{" transition_item* "}"

space_decl
  = "space" IDENT "{" space_item* "}"

region_stmt
  = place_stmt
  | scatter_stmt
  | overlay_stmt
  | replace_stmt
  | if_stmt

place_stmt
  = "place" IDENT "=" expr

overlay_stmt
  = "overlay" IDENT "=" expr

replace_stmt
  = "replace" IDENT "=" expr

scatter_stmt
  = "scatter" IDENT "{" scatter_item* "}"

space_item
  = residency_decl
  | dynamic_decl

residency_decl
  = "streamed" IDENT ":" residency_type residency_config*

dynamic_decl
  = "dynamic" IDENT ":" residency_type "using" IDENT residency_config*
```

#### Domain, Render, And Capture Grammar

```ebnf
domain_decl
  = "domain" IDENT param_list "{" domain_item* "}"

render_decl
  = "render" IDENT param_list "{" render_item* "}"

capture_expr
  = "capture" IDENT
```

#### Test Grammar

```ebnf
test_decl
  = "test" IDENT "(" ")" block
```

### Name Resolution And Lane Boundaries

Rules:

- Portable declarations MAY reference:
  - portable values
  - portable enums
  - portable phases
  - portable stdlib intrinsics
  - other portable declarations
- Portable declarations MUST NOT reference:
  - `resource`
  - `system`
  - host-only `fn`
  - async or actor constructs
- Host declarations MAY reference portable declarations.
- A call from host code into portable code is legal.
- A call from portable code into host code is illegal.
- `test` declarations are host-lane declarations.
- `test` declarations MAY reference host declarations, portable declarations, domains, renders, and testing intrinsics.
- Production declarations MUST NOT depend on `test` declarations.
- The compiler MUST model this as an acyclic host-to-portable dependency boundary.

## Portable Type System

### Scalar Types

Portable declarations MUST support the following explicit-width scalar types:

- `Bool`
- `I32`
- `I64`
- `U32`
- `U64`
- `F16`
- `F32`

The following existing surface types MUST NOT be legal in portable declarations:

- `Integer`
- `Float`
- `Number`

The host lane MAY continue to support existing scalar types, but portable declarations MUST require explicit-width numerics.

### Composite Portable Types

Portable declarations MUST support:

- `Vec2`
- `Vec3`
- `Vec4`
- `Mat3`
- `Mat4`
- `Quat`
- `Bounds2`
- `Bounds3`
- `Ray3`
- `Transform3`

The standard library SHALL define canonical `value` types at minimum for:

- `Surface`
- `Medium`
- `Hit3`
- `Payload`
- `Support3`
- `Contact`
- `Light`
- `Camera`
- `ActorHandle`

### `value`

`value` is the canonical portable aggregate type.

Rules:

- `value` MUST be fixed-layout POD.
- `value` MUST NOT carry identity.
- `value` MUST NOT allocate.
- `value` fields MUST themselves be portable types.
- `value` MUST be valid for CPU/GPU ABI transport.

Example:

```wr
value Surface {
    albedo: Vec3
    roughness: F32
    metalness: F32
    clearcoat: F32
    clearcoat_roughness: F32
    sheen: F32
    emissive: Vec3
}
```

### Forbidden Portable Types And Features

Portable declarations MUST NOT use:

- strings
- maps
- lists
- classes with identity
- interfaces with dynamic dispatch
- actors
- pending values
- host resources
- async operations
- IO
- closures that capture host state

If a declaration needs dynamic data, it MUST receive that data through fixed-layout `value` parameters or typed container interfaces defined by this RFC.

### Arrays And Fixed-Length Sequences

Portable declarations SHOULD support fixed-length arrays:

```wr
[T; N]
```

Rules:

- `N` MUST be compile-time constant.
- `T` MUST be portable.
- Fixed-length arrays are legal in `value` and portable declaration signatures.
- Variable-length arrays are not part of this RFC.

### Portable Handles

Stable gameplay identity used by portable queries SHOULD be represented explicitly using `value` types such as:

```wr
value ActorHandle {
    id: U64
    generation: U32
}
```

Rules:

- Portable handles MUST be opaque to the portable lane except for equality and payload transport.
- Handle allocation and liveness are host responsibilities.
- Dynamic containers MUST use stable handles rather than transient indices.

## Host Type System Additions

### `phase`

`phase` defines a finite authored world-state type intended for coarse specialization and world composition.

Example:

```wr
phase TownPhase {
    intact
    raided
    rebuilt
}
```

Rules:

- `phase` values MUST be finite and enumerable.
- `phase` MUST be usable in `region`, `transition`, and `space` composition.
- `phase` SHOULD be used to describe coarse world-state branches, not fine-grained per-sample logic.
- The compiler MAY specialize region composition by `phase`.

### `resource`

`resource` defines mutable host-owned state.

Rules:

- `resource` MAY be read/written by `system`.
- `resource` MUST NOT be read or written directly by portable declarations.
- Portable declarations may only observe resource-derived state through captured `value` data.

## Purity And Effects

### Host Lane

Host lane declarations include:

- `fn`
- `system`
- host-side portions of runtime setup and orchestration

Host lane code MAY:

- mutate resources
- perform IO
- use async features
- coordinate streaming
- construct captures

### Portable Lane

Portable lane declarations include:

- `kernel fn`
- `field`
- `material`
- `radiance field`
- `volume field`
- portable expressions inside `generator`, `archetype`, `body`, `move`, and `render`

Portable lane code MUST be pure.

Portable lane code MUST NOT:

- mutate state
- perform IO
- call host-only functions
- allocate dynamic host-owned structures
- use async or concurrency
- access resources directly

Portable purity MUST be enforced by the compiler.

### Portable Control Flow

Portable declarations MAY use:

- `if`
- `match`
- bounded `for`
- bounded `while`
- local immutable and mutable bindings

Portable declarations MUST NOT use unbounded host-driven iteration over runtime collections unless the collection is a compiler-known container abstraction from this RFC and the operation is explicitly supported.

Examples of legal structured iteration:

- reduction over a fixed-length array
- compiler-known scatter-slot evaluation
- compiler-known local archetype part iteration

Examples of illegal portable iteration:

- arbitrary traversal of `resource`-owned maps
- host iterator protocols
- dynamic filesystem or network enumeration

## Spatial Declaration Semantics

### `field`

The language SHALL support two distance field classes:

```wr
field exact distance name(...) -> F32 { ... }
field conservative distance name(...) -> F32 { ... }
```

Rules:

- `exact distance` MUST satisfy the language's exact signed-distance contract.
- `conservative distance` MUST return a safe lower bound for traversal and pruning.
- The compiler MUST diagnose operations that invalidate the declared field class.
- The language MAY support additional field classes in later RFCs, but `exact distance` and `conservative distance` are the minimum required classes.

### Exactness Contract

For `field exact distance`, the implementation MUST define a closed operation subset that preserves exact signed-distance behavior.

The exact subset MUST include at minimum:

- sphere-like primitives
- plane-like primitives
- capsule-like primitives
- rigid transforms
- uniform scaling with explicit correction
- exact-preserving `min`/`max` cases

The exact subset MUST explicitly reject or degrade exactness for operations such as:

- nonuniform scaling without proof
- arbitrary displacement
- uncontrolled warps
- smoothing operators that break exactness
- non-Lipschitz compositions

The compiler MUST either:

- prove the exactness contract holds, or
- reject the declaration, or
- require it to be rewritten as `conservative distance`

### Local-Space Evaluation

Spatial declarations are authored in local space unless explicitly stated otherwise.

Rules:

- `p` in a field body is interpreted in the declaration's local evaluation frame.
- `transform = ...` on an archetype defines how world queries are transformed into that local frame.
- The runtime MUST evaluate supports and geometry in a manner consistent with this transform contract.
- Normals and hit positions MUST be transformed back into the requested query frame.

### `material`

`material` computes the appearance of a known surface hit.

Rules:

- `material` MUST NOT define geometry.
- `material` MUST receive at least a `Hit3`.
- `material` returns `Surface`.
- `material` MUST be pure and portable.
- `material` SHOULD use filtered procedural operations for stable distance rendering across pixel footprints.

### `radiance field`

`radiance field` computes emitted light contributions that are not surface-local materials.

Examples:

- sky emission
- boss glow
- glyph light
- energy seams

Rules:

- `radiance field` returns `Vec3`.
- `radiance field` MUST be portable and pure.
- The render domain MAY integrate radiance fields differently from materials or media.

### `volume field`

`volume field` computes volumetric medium properties.

Rules:

- `volume field` returns `Medium`.
- `Medium` MUST include at least `density`, `emission`, and `anisotropy`.
- `volume field` participates in domains that enable media.

## Detail Levels

Portable spatial declarations MUST support explicit detail layers.

Minimum required detail layers:

- `coarse`
- `fine`

These appear inside `generator`, `archetype`, and `body`, for example:

```wr
detail coarse field exact distance(p: Vec3) -> F32 { ... }
detail fine field conservative distance(p: Vec3) -> F32 { ... }
```

Rules:

- `coarse` MUST be cheap enough for collision, visibility, and broad traversal.
- `fine` MAY be more expensive and presentation-oriented.
- Domain definitions MUST be able to select detail layers explicitly.
- The compiler MUST reject declarations that refer to undefined detail layers.

### Domain-Specific Layer Exports

Composite spatial declarations MAY export multiple semantic geometry layers, including:

- `geometry`
- `terrain`
- `collision`

Rules:

- `geometry` is the default visual/spatial surface contribution.
- `terrain` is intended for walkable or deformable ground participation.
- `collision` is intended for physical contact if different from visual geometry.
- Domains MAY select one or more layers when evaluating the world.

## Support Metadata

Every nontrivial spatial declaration intended for world composition MUST expose support metadata.

Minimum required support kinds:

- `support`
- `coarse_support`
- `tight_support`

Semantics:

- `support` is the generic support contract.
- `coarse_support` is broadphase-oriented and cheap.
- `tight_support` is a tighter support approximation for finer pruning.

Rules:

- The compiler MUST use supports to build query candidate sets.
- World-scale evaluation MUST NOT scan all regions or instances linearly.
- Support computations MUST be portable or host-computable from portable data.

### `Support3`

The standard library SHALL define `Support3` as an abstract support contract.

Implementations MAY lower `Support3` to:

- axis-aligned bounds
- oriented bounds
- capsules
- wedges
- unions of simple primitives

Rules:

- `Support3` MUST be finite.
- Streamable declarations MUST have finite support.
- Infinite-support authored declarations MUST NOT be admitted into streamed composition directly.

## `generator`

`generator` defines parameterized field-authored content with no runtime identity.

Example:

```wr
generator OakTree(seed: U64, age: F32) {
    support = ...
    detail coarse field exact distance(p: Vec3) -> F32 { ... }
    detail fine field conservative distance(p: Vec3) -> F32 { ... }
    material surface(hit: Hit3) -> Surface { ... }
}
```

Rules:

- `generator` MUST be parameterized only by portable values.
- `generator` MUST NOT imply runtime identity.
- `generator` MAY export geometry, materials, radiance, and media.
- `generator` MAY be used by `region` composition and `scatter`.
- `generator` MAY be instantiated many times from seeds and parameters.

### Generator Determinism

Given identical parameters and identical stdlib/compiler versions, a generator SHOULD be deterministic in its portable outputs. Host-driven nondeterminism MUST NOT leak into generator evaluation.

## `archetype`

`archetype` defines a reusable field family driven by runtime state.

Example:

```wr
archetype Enemy(instance: EnemyState) {
    transform = instance.transform
    coarse_support = ...
    tight_support = ...
    detail coarse field exact distance(p: Vec3) -> F32 { ... }
    detail fine field conservative distance(p: Vec3) -> F32 { ... }
    material surface(hit: Hit3) -> Surface { ... }
    payload = ...
}
```

Required exports:

- a transform or local frame
- support metadata
- at least one geometry field
- optional `material`
- optional `radiance field`
- optional `volume field`
- optional `payload`
- optional `terrain` field contribution

Rules:

- `archetype` MUST bind to instance state through a portable `value`.
- `archetype` MUST remain reusable across many instance descriptors.
- The compiler MUST compile the archetype bundle once and reuse it across instances.
- Dynamic state updates MUST mutate descriptors, not the archetype code.

### `deform`

`archetype` and `body` MAY expose:

```wr
deform = expr
```

Semantics:

- `deform` is a portable transformation or deformation mapping applied to the declaration's local evaluation frame.
- `deform` is intended for articulated or otherwise dynamic shape changes, including skeletal-like rigs.
- `deform` MUST be compiler-visible; opaque host callbacks are not allowed.

Rules:

- `deform` MUST be expressed as portable data and portable operations.
- The compiler MAY require support metadata that over-approximates the deformed shape.
- Domains MAY choose to evaluate deformed coarse and fine layers differently.

### `payload`

`payload` attaches portable identity or material metadata to a hit contribution.

Rules:

- `payload` MUST be a portable `value`.
- `payload` MUST be query-visible through hit results.
- `payload` MUST NOT encode host references or raw pointers.

## `body`

`body` defines a dynamic physical object with field collision representation.

Example:

```wr
body KatanaBody(instance: BladeState) {
    mass = 2.6
    inertia = blade_inertia(...)
    collision detail exact distance(p: Vec3) -> F32 { ... }
    material surface(hit: Hit3) -> Surface { ... }
}
```

Rules:

- `body` MUST expose physical mass properties.
- `body` MUST expose collision geometry as a field.
- `body` MAY expose materials for presentation.
- `body` MUST be compatible with the move solver and collision domain.

### Collision And Contact Surface

The runtime query API MUST support body-related queries including:

- swept collision against a domain
- overlap/contact generation
- nearest-hit queries against collision layers

`body` collision fields participate in these queries via `collision` exports and support metadata.

## Physics-Authored Moves

### `move`

`move` defines a deterministic, time-bounded control program over one or more bodies.

Example:

```wr
move DrawSever(player: PlayerState, blade: BladeState, target: TargetState) {
    duration = 0.54

    phase gather[0.00..0.16] { ... }
    phase cut[0.16..0.32] { ... }
    phase recover[0.32..0.54] { ... }
}
```

Rules:

- `move` MUST be authored in terms of constraints, targets, impulses, springs, damping, and contact windows.
- `move` MUST NOT be an animation clip.
- `move` MUST be predefined and locked to authored semantics.
- `move` MUST be deterministic for fixed timestep evaluation.
- `move` MAY use portable math helpers through `kernel fn`.

The language SHOULD standardize portable move operators including:

- `anchor`
- `orbit`
- `align`
- `spring`
- `damp`
- `impulse`
- `sweep`
- `lead`
- `clamp`
- `contact`
- `window`

These operators are part of the authored move DSL and SHOULD lower into deterministic host/runtime solver logic.

### Solver Semantics

The implementation MUST define one deterministic solver contract for `move` execution.

At minimum, the solver contract MUST specify:

- fixed timestep stepping rules
- phase boundary handling when a timestep crosses a phase edge
- operator evaluation order within a phase
- constraint priority rules when multiple operators interact
- collision and continuous-contact handling for bodies participating in a move
- tie-breaking rules for simultaneous contacts or simultaneous legal transitions
- deterministic numeric tolerances used by contact and window tests

The default solver model SHOULD be:

1. sample the active move phase for the current fixed timestep interval
2. evaluate move operators in source order unless a stronger priority class is defined by the language
3. solve constraints and impulses against participating bodies
4. resolve collision and contact against the active collision domain
5. emit contacts and move transition signals for the completed step

Two conforming implementations MUST NOT be free to produce materially different move outcomes for the same initial state, timestep, and stdlib/compiler/runtime version.

### `moveset`

`moveset` defines move selection and legal transitions.

Example:

```wr
moveset KatanaArts {
    idle = OrbitGuard
    on light_attack when Targeting.state.locked_on => DrawSever
    on parry => TetherCounter
}
```

Rules:

- `moveset` MUST map intent plus state predicates to allowed moves.
- The host solver MUST use `moveset` legality for move transitions.
- The compiler MUST ensure referenced moves exist and signatures are compatible.

### Intent Input

This RFC assumes intent is represented in host state and consumed by the moveset solver. Intent transport is not itself portable content, but move legality expressions MUST remain side-effect free and analyzable.

## Region Composition

### `region`

`region` defines a streamable unit of world composition.

Example:

```wr
region Highlands(key: CellKey, seed: U64, phase town: TownPhase) {
    terrain = HighlandsTerrain(key=key, seed=seed)
    scatter trees { ... }
    if town == intact { ... }
}
```

Rules:

- `region` parameters MUST be portable values.
- `region` MAY `place`, `scatter`, `overlay`, `replace`, and branch on `phase`.
- `region` MUST be finite and streamable.
- `region` MUST expose support metadata derivable from its contents.

### Region Statements

`region` statement semantics:

- `place` introduces a named field contribution.
- `overlay` unions or accumulates above previous lower-priority content.
- `replace` masks and supersedes lower-priority content in an overlap zone.
- `scatter` creates many derived placements from seeds, masks, and rules.
- `if` performs coarse structural branching. It MUST NOT imply per-sample branch explosion.

The compiler MAY lower region statements into region-local composition plans rather than direct expression trees.

### Composition Conflict Resolution

Region and space composition MUST define deterministic conflict resolution semantics.

At minimum, the implementation MUST resolve conflicts using the following order:

1. nearest valid geometric contribution by domain-relevant distance
2. explicit composition tier priority
3. explicit declaration priority if the language later adds one
4. source order within the same composition tier
5. stable declaration identity as the final tie-breaker

Rules:

- `replace` MUST mask lower-priority geometry contributions inside its valid overlap zone
- when a geometry contributor wins, its payload and material ownership wins unless explicitly forwarded or blended by the active composition rule
- radiance and media participation MAY accumulate even when geometry ownership does not
- if a transition or replacement rule blends geometry, the payload ownership rule MUST still be defined deterministically

The implementation MUST NOT leave equal-distance or overlapping-contributor behavior implementation-defined.

### `transition`

`transition` defines a first-class overlap region between region families.

Example:

```wr
transition PlainsToMountain(left: PlainsCell, right: MountainCell, seed: U64) {
    geometry = blend(...)
    material = blend(...)
}
```

Rules:

- transitions MUST be explicit, not hidden inside giant terrain functions
- transitions MAY blend geometry, materials, media, or radiance differently
- transitions MUST be streamable like regions

Transitions SHOULD be used for:

- biome boundaries
- region seams
- landing insertion
- impossible geometry handoffs
- portal threshold effects

### `place`, `scatter`, `overlay`, `replace`

The region composition language MUST include at minimum:

- `place`: inject one named composed object
- `scatter`: distribute many generated instances over a support/mask
- `overlay`: add or union a layer without replacing prior content
- `replace`: replace a lower-priority layer in an overlap zone

Semantics:

- `place` is explicit one-off composition.
- `scatter` is procedural repeated placement from seeds and rules.
- `overlay` is additive or union composition.
- `replace` is priority composition for landings, encounter arenas, and phase swaps.

### Scatter Semantics

`scatter` MUST be seed-driven and compiler-visible.

Rules:

- scatter placement MUST be derivable from inputs such as masks, density, slots, and seeds
- scatter MUST NOT depend on imported asset lists
- scatter MAY lower into runtime descriptor generation plus derived support structures
- scatter MUST provide stable placement under fixed seeds unless explicit region or phase parameters change
- scatter ordering and generated handle identity MUST NOT depend on residency order
- scatter neighborhood dependencies MUST be explicit and bounded
- scatter generation for one region MUST be reproducible without requiring the full world to be resident

## World Topology

The language SHALL define topology-aware region containers.

Required built-ins:

- `RegionLine[T]`
- `RegionGrid[T]`
- `RegionGraph[T]`
- `DynamicSpatial[T]`
- `StaticSpatial[T]`
- `Singleton[T]`
- `SparseBands[T]`
- `BoundarySet[T]`

Semantics:

- `RegionLine[T]` is for 1D progress worlds such as the `Staircase` ascent.
- `RegionGrid[T]` is for 2D open worlds such as plains to mountains.
- `RegionGraph[T]` is for portals, dungeons, and hub graphs.
- `DynamicSpatial[T]` is for moving runtime instances addressed by stable handles.
- `StaticSpatial[T]` is for procedurally fixed but query-accelerated region content.
- `Singleton[T]` is for unique dynamic entities such as the katana.
- `SparseBands[T]` is for authored sparse encounter insertions along a line topology.
- `BoundarySet[T]` is for explicit transition regions at region family boundaries.

Rules:

- Region containers MUST stream descriptors, not authored assets.
- Dynamic containers MUST use stable handles, not transient indices, for gameplay identity.
- The runtime MUST use supports and topology structure to derive query candidate sets.

### Residency Semantics

Residency declarations MAY include properties such as:

- `radius`
- `follow`
- `around`
- `where`
- `using`

Implementations MUST define these as descriptor residency policies rather than source-level content duplication.

## `space`

`space` defines the composed world schema and residency strategy.

Example:

```wr
space StaircaseWorld {
    streamed bands: RegionLine[StairBand] radius 6 follow Player.state.band
    dynamic katana: Singleton[BladeState] using KatanaBody
}
```

Rules:

- `space` MUST define composition order across static regions, transitions, dynamic archetypes, and effects.
- `space` MUST be capturable into `Capture[World]`.
- `space` MUST declare residency topology and runtime-follow behavior where applicable.
- The compiler MUST lower `space` into descriptor-table plans and query reducers, not a flattened mega-field.

### Composition Order

`space` MUST define deterministic composition order. The minimum required order model is:

1. base static regions
2. transition regions
3. replacement regions
4. dynamic terrain contributions
5. dynamic geometry contributions
6. radiance and media contributions

The implementation MAY refine this model, but composition order MUST be explicit and deterministic.

Within each tier, the implementation MUST define deterministic contributor ordering and conflict resolution as described in `Composition Conflict Resolution`.

## Capture Model

The language SHALL add:

```wr
Capture[World]
capture World
```

Semantics:

- `Capture[World]` is an immutable query epoch over a composed `space`.
- `capture World` freezes descriptor tables and relevant resource-derived state for querying.
- `capture` MUST use structural sharing and MUST NOT imply a deep copy.
- Domains and renders MUST query captures, not mutable resources directly.

Every capture MUST carry a stable epoch identity.

### Capture Lifetime

Captures MAY be frame-local or solver-local. A capture SHOULD be cheap to construct relative to world scale because it reuses descriptor storage structurally.

The runtime MUST define explicit lifetime and retirement semantics for captures.

At minimum:

- a capture MUST observe one coherent descriptor/state snapshot
- later resource mutation MUST NOT retroactively change an existing capture
- any CPU or GPU work submitted against a capture MUST retain that capture until the work is complete
- descriptor storage referenced by an in-flight capture MUST NOT be reclaimed or mutated in place
- capture retirement MUST be synchronized against CPU and GPU completion fences or equivalent completion signals

The runtime MAY implement captures through reference counting, arena epochs, generations, fence-tracked retirement, or equivalent mechanisms, but the observable semantics MUST remain coherent and race-free.

## Domains

The language SHALL add:

```wr
domain Name(world: Capture[World], ...) { ... }
```

Required domain properties:

- selected geometry detail level
- whether material evaluation is enabled
- whether radiance evaluation is enabled
- whether media evaluation is enabled
- max distance or budget controls as needed
- error tolerance configuration
- topology or frame-of-reference semantics where relevant

Example:

```wr
domain Collision(world: Capture[StaircaseWorld]) {
    geometry_detail = coarse
    material = false
    radiance = false
    media = false
}
```

Rules:

- gameplay systems MUST use non-presentation domains where appropriate
- domains MUST control evaluation cost and semantics
- the compiler MAY specialize query plans by domain

Domains that expose navigation or walkability semantics MUST also define:

- the surface frame or local up convention used for walkability
- adjacency semantics across regions, transitions, and impossible-geometry handoffs
- whether traversal is Euclidean, topological, or hybrid
- the acceptance thresholds for slope, clearance, and support continuity

### Required Query Surface

The runtime and stdlib MUST expose domain-aware query APIs over captures, including at minimum:

- `nearest(world=..., ray=..., domain=..., max_distance=...)`
- `occluded(world=..., ray=..., domain=..., max_distance=...)`
- `distance_at(world=..., point=..., domain=...)`
- `normal_at(world=..., point=..., domain=...)`
- `walkable(world=..., point=..., domain=...)`

### Navigation And Impossible Geometry Semantics

Navigation and walkability MUST NOT be treated as implicit byproducts of Euclidean distance alone.

For domains that support navigation, the implementation MUST model traversal using:

- explicit surface frames or local-up conventions
- explicit adjacency between regions and transitions
- deterministic continuity rules across folds, portals, and other impossible-geometry handoffs

`walkable(...)` MUST be defined relative to the active navigation domain semantics, not merely by checking whether a point lies near a surface.

If the world contains topological handoffs that are non-Euclidean, the navigation domain MUST use topology-aware adjacency rather than raw world-space nearest-neighbor assumptions.
- `overlap(body=..., world=..., domain=...)`

These APIs MUST return portable result values suitable for CPU tests and gameplay logic.

## `render`

`render` defines presentation configuration over a captured world and camera.

Example:

```wr
render StaircaseView(world: Capture[StaircaseWorld], camera: Camera) {
    domain = Presentation(world=world, camera=camera)
    lights = [ ... ]
    radiance = [ ... ]
    media = [ ... ]
    limits = render_limits(...)
}
```

Rules:

- `render` MUST be declarative
- `render` MUST NOT expose hand-authored march loops as the primary abstraction
- `render` MUST allow:
  - lights
  - radiance fields
  - media fields
  - error/quality limits
  - output attachments
  - post chain configuration
- runtime frame attachments and history buffers MAY exist as transient execution resources

### Presentation Outputs

`render` SHOULD be able to target named outputs such as:

- color
- depth
- motion
- luminance history
- debug visualizations

These outputs are transient runtime products and not authored assets.

## Materials, Lighting, And Shine

The language MUST treat materials, radiance, and media as first-class authored layers.

### Material Requirements

The standard material model MUST support at least:

- diffuse/albedo
- roughness
- metalness
- clearcoat
- clearcoat roughness
- sheen
- emissive

### Procedural Filtering

Because there are no texture assets, the standard library MUST provide filtered procedural primitives for:

- filtered noise
- filtered stripe/grid patterns
- filtered marbling
- derivative-aware layer masks

The compiler and stdlib MUST provide the means to avoid severe shimmering in distant or glancing-angle procedural materials.

### Required Procedural Material Library

The standard library SHOULD include:

- filtered noise families
- filtered cellular patterns
- filtered banding and stripes
- slope-aware blends
- curvature-aware wear
- triplanar-like field projections expressed without texture assets

### Lighting Layers

The render model MUST allow:

- directional lights
- local procedural lights
- emissive radiance fields
- sky radiance
- volumetric media
- bloom/glare integration over bright procedural highlights

### Media And Light Interaction

The implementation SHOULD support the interaction between:

- lights and media
- radiance fields and media
- emissive materials and media

This is necessary for the "shine" and atmospheric goals described by the design target.

## Streaming And Chunking Model

### Fundamental Rule

A large world is not one infinite field. It is an indexed family of finite field regions.

### Descriptor Streaming

Streaming MUST move region descriptors, not authored assets.

A region descriptor SHOULD minimally contain:

- region key
- seed
- local frame transform
- support metadata
- phase tags
- references to compiled generator/archetype bundles

### Region-Local Frames

Every streamed region descriptor SHOULD carry:

- `world_from_local`
- `local_from_world`

This is REQUIRED for large-world precision and for authored local-space fields.

### `Staircase`

`Staircase` MUST use `RegionLine[StairBand]`.

Each band:

- is finite
- is authored in local space
- has a transform into world space
- is streamable by band index
- may be overlaid or replaced by landing/encounter regions

### Open World RPG

An open world MUST use `RegionGrid` or `RegionGraph` composition across biome and topology families. Transition regions SHOULD be first-class instead of hidden inside monolithic terrain fields.

## Standard Library Requirements

The implementation MUST ship a portable/game stdlib sufficient to author the target content. At minimum it SHALL include:

- vector, matrix, and quaternion math
- rigid transforms
- signed-distance primitives
- exactness-preserving combinators
- conservative field combinators
- filtered procedural noise
- support and bounds constructors
- lighting helpers
- media helpers
- move and constraint math helpers
- domain query result types
- capture and query helper functions
- testing and differential comparison helpers

The exact surface MAY evolve, but the presence of this capability set is REQUIRED.

## Compiler Architecture

### HIR Changes

HIR MUST add explicit node kinds for:

- `ValueDecl`
- `PhaseDecl`
- `ResourceDecl`
- `KernelDecl`
- `FieldDecl`
- `MaterialDecl`
- `RadianceDecl`
- `VolumeDecl`
- `GeneratorDecl`
- `ArchetypeDecl`
- `BodyDecl`
- `MoveDecl`
- `MovesetDecl`
- `RegionDecl`
- `TransitionDecl`
- `SpaceDecl`
- `DomainDecl`
- `RenderDecl`
- `TestDecl`
- `CaptureExpr`

### Whole-Program Semantic Layer

The compiler MUST maintain one unified whole-program semantic representation before lowering into execution-specific IRs.

This RFC refers to that representation as the typed HIR or semantic program layer. The exact name MAY vary, but the semantic role is REQUIRED.

That whole-program semantic layer MUST be the single source of truth for:

- declaration identity
- typing
- purity and effect legality
- lane membership
- call graph structure
- dependency graph structure
- domain participation
- capture/resource boundaries
- specialization eligibility
- declaration reachability
- payload and layout closure

The implementation MUST NOT fork host and portable semantics before this whole-program stage is established.

#### Purpose

The purpose of the whole-program semantic layer is to preserve whole-program analysis and optimization while still allowing distinct host and portable execution IRs later in the pipeline.

This means the compiler architecture SHOULD be understood as:

1. parse into syntax
2. resolve into one typed whole-program semantic representation
3. run whole-program analysis and specialization planning
4. lower into host IR and portable IR
5. run lane-specific optimization and code generation

Having multiple execution IRs does not violate the whole-program optimization goal as long as the whole-program semantic layer remains unified.

### Portable IR

The compiler MUST introduce a dedicated portable IR rather than reusing the current general MIR unchanged.

Portable IR MUST encode:

- explicit-width scalar types
- vector and matrix types
- fixed-layout aggregates
- pure call graphs
- field class (`exact`, `conservative`)
- support metadata
- detail layers
- domain-facing exports
- local-to-world/world-to-local transform contracts

Portable IR MUST NOT encode host effects.

### Host IR

The compiler MAY continue to use the existing host-oriented MIR or an equivalent host execution IR for the host lane.

Whatever representation is used for host lowering MUST be distinct in purpose from the portable IR.

Host IR is responsible for representing:

- mutation
- resource orchestration
- scheduling
- runtime calls
- capture construction
- platform/runtime integration

Portable IR is responsible for representing:

- pure portable computations
- field and domain exports
- fixed-layout values
- support/detail metadata
- GPU-compatible semantics

The compiler MUST NOT force host and portable lowering into one shared low-level execution IR if doing so would erase the semantic constraints required by either lane.

### Analyses

The compiler SHOULD implement at minimum:

- purity checking
- portable type legality checking
- exactness/conservative-field validation
- support-bound inference or validation
- detail-level export validation
- domain compatibility checking
- archetype/generator bundle summarization
- phase specialization eligibility
- local-space transform validation
- scatter stability validation
- navigation-domain legality validation
- composition conflict resolution validation

Whole-program analyses SHOULD also include:

- declaration reachability for stripping and backend selection
- cross-domain backend eligibility analysis
- constant propagation across declaration boundaries where legal
- phase specialization planning
- capture dependency closure
- generated metadata closure for reachable declarations

### Backend Strategy

The compiler MUST emit from the same portable IR:

- CPU-native code for tests, queries, tools, and fallback
- GPU-native programs for presentation

The compiler MUST support per-domain backend emission rather than one global "CPU on/off" switch.

Whole-program optimization decisions that affect both host and portable lowering MUST occur before the host/portable split or must be preserved explicitly as summaries flowing into both lower IRs.

In particular:

- declarations required by non-presentation domains such as collision, navigation, visibility, and gameplay authority MUST be eligible for CPU emission in production
- declarations used only by presentation MAY omit production CPU emission
- testing builds MUST preserve CPU-reference execution for all portable declarations
- production builds MAY strip unused CPU variants for declarations that are presentation-only

The implementation MUST distinguish:

- host native code
- portable CPU code
- portable GPU code

These are distinct semantic products even if they ultimately reside in one executable image.

#### Host Native Code Versus Portable CPU Code

Host native code is the compiled form of the full host lane and is responsible for:

- mutation
- resource orchestration
- streaming
- scheduling
- capture construction
- input, audio, and other runtime integration
- save/load and platform integration

Portable CPU code is the CPU-native compiled form of the portable lane and is responsible for:

- field evaluation
- material evaluation
- radiance and media evaluation
- support queries
- domain queries
- deterministic shared math used by both CPU and GPU semantics

Portable CPU code MUST remain governed by the same purity and layout rules as the portable lane source. It is not a second host language.

#### Generated Integration Metadata

The implementation MUST generate static integration metadata and glue for the compiled program.

This RFC does not require a general-purpose reflection system. Instead, it requires compiler-generated metadata sufficient to bind:

- host code to portable CPU entrypoints
- host runtime to GPU modules
- captures to domain evaluators
- payload schemas to query and render consumers
- detail-level exports to domain planners

At minimum, generated integration metadata MUST describe:

- declaration identities
- exported layers and detail levels
- payload layouts
- domain participation
- capture/world layout bindings
- render entrypoint bindings
- CPU symbol bindings
- GPU module/pipeline bindings

Implementations MAY realize this as static tables, generated Rust modules, generated C-like glue, or equivalent compiled artifacts.

#### Execution Modes

The implementation SHOULD support three execution modes for portable programs:

1. reference CPU execution for truth, debugging, and tests
2. optimized CPU-native execution for gameplay/query domains
3. GPU-native execution for presentation and optionally GPU-side batch queries

The reference CPU execution path MAY be interpreted or compiled, but it MUST preserve clear, debuggable semantics suitable for testing.

The compiler MAY choose evaluation strategies per domain and per declaration bundle, including:

- analytic intersection when provable
- interval pruning
- bounded stepping
- sphere tracing
- root refinement
- GPU-side batch query evaluation

The source language MUST NOT force one low-level evaluation strategy.

#### Reference Implementation Guidance

The reference implementation of this RFC SHOULD be built in Rust.

Recommended stack:

- custom `wrela` parser, HIR, portable IR, and field/domain optimizer
- Rust runtime for streaming, captures, query execution, and integration
- Cranelift or equivalent mature code generation library for native CPU code
- WGSL as the first GPU target language
- `wgpu` or equivalent cross-platform graphics runtime for GPU integration
- shader validation/translation infrastructure such as `naga` as a helper layer rather than the primary language IR

The implementation SHOULD NOT attempt to own:

- machine-code generation from first principles
- register allocation
- object file writing from first principles
- graphics driver interfaces from first principles
- vendor-specific graphics backend maintenance

The value of the project is in the language, portable IR, field optimizer, and runtime architecture, not in rebuilding existing systems programming toolchains.

## Runtime Architecture

The runtime MUST provide:

- a descriptor registry for resident regions and dynamic instances
- a compiled bundle registry for generators/archetypes/bodies
- capture construction
- domain-aware query execution
- streaming residency management
- derived execution artifact management
- generated metadata consumption and binding
- GPU module and pipeline initialization from embedded program artifacts
- capture epoch tracking and retirement
- bounded specialization and artifact cache management

The runtime MUST distinguish:

- authored declarations
- live descriptors/state
- derived execution artifacts
- generated integration metadata

Dynamic movement MUST update descriptors and spatial indices, not rewrite authored field source.

The runtime MUST use generated integration metadata to coordinate:

- domain query dispatch
- capture layout interpretation
- portable CPU entrypoint invocation
- GPU module/pipeline creation
- payload decoding and result marshaling

The runtime SHOULD treat portable CPU and GPU programs as peers produced from one portable source rather than as hand-maintained separate implementations.

The runtime MUST expose observability for:

- active capture epochs
- specialization cache entries
- artifact memory usage
- residency contents
- pipeline creation and reuse

## Tooling Requirements

The implementation SHOULD ship with:

- field previews
- region composition previews
- support and residency visualizers
- domain cost visualizers
- move/physics replay tools
- deterministic CPU image/query test harnesses
- CPU/GPU query differential tools
- CPU/GPU render differential tools with image diff output
- seed/time freezing helpers for tests
- failure minimization and shrinking aids for query-driven tests where feasible
- capture epoch and lifetime visualizers
- specialization budget and cache inspectors
- navigation topology visualizers
- diagnostics that explain why a declaration is not portable or not exact

## Diagnostics

The compiler MUST produce high-signal diagnostics for at least:

- use of forbidden types in portable declarations
- calls from portable code into host code
- missing support metadata for streamable declarations
- invalid field-class claims
- detail-level mismatches
- invalid domain references
- invalid `moveset` references
- illegal resource access in portable code
- illegal mutation in portable code
- region composition cycles or unsupported overlap rules
- unstable scatter dependencies
- invalid or underspecified navigation domain semantics
- unbounded or unsupported specialization triggers

Diagnostics SHOULD include fix guidance where possible.

## Build And Packaging

`wrela build` MUST continue to emit a standalone native binary.

The standalone artifact is one native program per target platform, not one universal binary across all platforms and GPU driver stacks.

The binary MUST bundle:

- host runtime code
- compiled portable CPU code selected for the chosen build profile
- compiled portable GPU programs or IR modules needed for the selected target backend
- world/render/domain metadata
- generated integration metadata and glue

No external authored asset pack is required by this RFC because authored content is field-only.

`wrela build` MUST NOT include `test` entrypoints in the production runtime entry graph by default.

The implementation MUST provide a separate test-oriented build or invocation path for executing `test` declarations.

#### Production Build Profiles

The implementation SHOULD support at least the following conceptual build profiles:

1. test/headless
2. shipping/full
3. shipping/presentation-heavy

`test/headless` SHOULD include:

- host code
- portable CPU reference execution
- portable CPU native execution where useful
- optional GPU execution for differential testing

`shipping/full` SHOULD include:

- host code
- GPU execution for presentation domains
- portable CPU code for authoritative and non-presentation domains

`shipping/presentation-heavy` SHOULD include:

- host code
- GPU execution for presentation domains
- only the portable CPU code required by authoritative or non-presentation domains
- stripping of portable CPU variants that are provably presentation-only
- bounded specialization and artifact budgets appropriate for shipping constraints

The exact naming of profiles MAY vary, but the capability set is REQUIRED.

#### Single-Binary Runtime Model

The preferred deployment model is:

- one native executable or native app bundle per target platform
- embedded host code
- embedded portable CPU code where selected
- embedded GPU program artifacts
- embedded metadata/glue needed to bind them together

At runtime, that single program initializes the graphics backend, loads its embedded GPU programs, binds its generated metadata, and coordinates host and portable execution internally.

The implementation MAY still depend on the platform graphics stack and drivers. This does not violate the single-binary deployment goal.

## Testing Requirements

The language implementation MUST make authored code directly testable.

Testing is not a bolt-on concern in this RFC. It is part of the semantic contract of the portable lane and part of the required implementation contract of the compiler, runtime, and tooling stack.

### Testing Goals

The testing model MUST provide:

- direct unit tests for portable functions and evaluators
- deterministic host-side tests for gameplay-facing systems
- world/query tests over captured spaces
- render/probe tests for presentation behavior
- first-class CPU/GPU differential testing
- actionable failure output with useful spatial and image diagnostics

The implementation MUST optimize for the developer experience that authored code can be tested without needing to hand-author GPU harnesses, shader inspection pipelines, or custom replay frameworks.

### Reference Semantics

The CPU backend is the reference execution lane for language-level correctness testing.

Rules:

- every portable declaration MUST lower to CPU-callable code
- the CPU backend defines the primary observable semantics for tests
- the GPU backend is a checked backend of the same portable program
- CPU and GPU are not required to be bitwise identical for floating-point presentation behavior
- CPU and GPU MUST agree within declared test tolerances for portable declarations and domain queries

This means authored code is tested primarily by running the portable program on CPU. GPU testing is differential, not the only avenue for correctness.

### Testing Surface

The language SHALL add:

```wr
test name() { ... }
```

`test` is a host-lane declaration with special runner semantics.

Rules:

- `test` MUST have no parameters
- `test` MUST execute in an isolated deterministic test context
- `test` MAY call host functions, portable declarations, domains, renders, and test intrinsics
- `test` MUST NOT be callable from production entry points
- tests MAY share helper `fn` declarations, but the test entry points themselves are not part of the production call graph

### Deterministic Test Context

Each `test` MUST execute with deterministic defaults unless explicitly overridden.

At minimum, the implementation MUST provide deterministic control over:

- time
- random seeds
- resource initialization
- move solver timestep
- capture construction
- residency state

The default test context SHOULD start from:

- a fixed clock value of zero
- a fixed RNG seed
- empty or zero-initialized mutable resources unless the test sets them
- no ambient asynchronous activity

The implementation MAY allow explicit overrides such as:

- fixed clocks
- fixed deltas
- explicit seeds
- explicit residency windows
- backend selection for differential tests

### Required Test Categories

The implementation MUST support at least the following categories:

1. unit tests for `kernel fn`
2. evaluator tests for `field`, `material`, `radiance field`, and `volume field`
3. `generator` and `archetype` bundle tests
4. domain query tests over `Capture[World]`
5. deterministic move solver tests
6. region composition tests
7. streaming residency tests
8. capture epoch and capture coherence tests
9. navigation and impossible-geometry topology tests
10. CPU render/probe tests
11. CPU/GPU differential query tests
12. CPU/GPU differential render tests

### Required Host-Side Test APIs

The standard library and runtime MUST expose host-callable APIs sufficient to test the language surface without custom harness code.

At minimum, the implementation MUST expose host-side operations equivalent to:

- evaluate a portable function directly
- sample a field or exported detail layer at a point
- evaluate a material at a hit
- evaluate a radiance field or volume field at a point
- query the support of a generator/archetype/body instance
- construct captures over spaces
- cast rays into a domain
- ask for nearest hits
- compute normals where valid
- simulate a move or moveset over a fixed timestep sequence
- render a frame or probe a render
- run CPU/GPU differential comparisons over queries and renders

Guide-level surface examples:

```wr
test enemy_support_contains_body() {
    enemy = enemy_fixture()
    support = support_of(archetype Humanoid(instance=enemy))

    expect contains(support, vec3(0.0, 1.0, 0.0))
    expect not contains(support, vec3(40.0, 40.0, 40.0))
}
```

```wr
test foldmother_terrain_raises_platform() {
    boss = foldmother_fixture(phase=BossPhase.phase_two, pressure=1.0, fold=0.6)

    base = sample terrain coarse of FoldMother(instance=boss) at vec3(0.0, 0.0, 0.0)
    raised = sample terrain coarse of FoldMother(instance=boss) at vec3(0.0, 2.0, 0.0)

    expect base < raised
}
```

```wr
test draw_sever_hits_locked_target() {
    result = simulate move DrawSever(
        player=player_fixture(),
        blade=blade_fixture(),
        target=locked_target_fixture(),
        duration=0.54,
        dt=1.0 / 120.0
    )

    expect result.contacts.count == 1
    expect result.contacts[0].kind == slash
}
```

The exact spelling of these intrinsics MAY vary, but equivalent capability is REQUIRED.

### Assertion Model

The language runtime and standard library MUST provide assertion forms suitable for exact and approximate testing.

At minimum, the implementation MUST support:

- boolean assertions
- exact equality assertions
- approximate scalar equality assertions
- approximate vector equality assertions
- payload equality assertions
- support containment assertions
- image comparison assertions
- structured query-result comparison assertions

Guide-level assertion examples:

```wr
expect hit.hit == true
expect exact(hit.payload.entity_id, 7_u64)
expect within(hit.distance, 4.125, abs=0.0005)
expect within_vec3(hit.normal, vec3(0.0, 1.0, 0.0), abs=0.002)
expect image_matches(actual=image, golden="boss_landing_phase_two", max_rms=0.01)
```

The implementation MAY spell these as functions instead of keywords, but it MUST provide the capability.

### Equality And Tolerance Semantics

Tests MUST distinguish exact and approximate assertions.

Rules:

- integers, booleans, enums, phases, handles, and payload identifiers SHOULD default to exact comparison
- floating-point values SHOULD default to approximate comparison only when explicitly requested
- exact field-class metadata and domain-routing behavior MUST be tested exactly
- normals, filtered materials, radiance, media, and full renders SHOULD be compared with declared tolerances

The implementation MUST support, at minimum:

- absolute tolerance
- relative tolerance
- per-channel image tolerance
- aggregate image tolerance such as RMS or max error

The implementation SHOULD support:

- masked image comparison regions
- histogram summaries
- difference image generation

### Portable Declaration Testing

Every portable declaration MUST be directly testable on CPU.

#### `kernel fn`

`kernel fn` MUST support ordinary deterministic unit testing with exact or approximate assertions depending on return type.

#### `field`

Fields MUST support host-side point sampling and query-driven testing.

Required capabilities:

- sample at a point
- query named detail levels
- query named exports such as `terrain`, `geometry`, or `collision`
- evaluate gradients or normals where defined
- verify exact versus conservative classification metadata

#### `material`

Materials MUST support host-side evaluation against explicit `Hit3` fixtures.

The implementation MUST make it possible to construct or derive the `Hit3` input needed to test a material independent of GPU execution.

#### `radiance field` And `volume field`

Radiance and volume fields MUST support host-side point sampling and domain-query participation tests.

### Generator, Archetype, And Body Testing

`generator`, `archetype`, and `body` declarations MUST expose testable contracts beyond their raw field exports.

The implementation MUST allow tests to inspect or validate:

- support metadata
- available detail levels
- payload behavior
- transform behavior
- deform application behavior
- collision exports for `body`
- terrain versus geometry export separation where present

Tests SHOULD be able to validate that an archetype or body:

- keeps its geometry within declared support
- exports required detail levels
- produces stable payload identity
- behaves deterministically for fixed inputs

### Capture And World Query Testing

Tests MUST be able to construct small deterministic worlds and query them without booting the full game loop.

The implementation MUST support:

- constructing minimal residency sets
- constructing dynamic instance sets
- capturing a `space` into `Capture[World]`
- executing domain queries against that capture

Required testable query classes include:

- nearest-hit tests
- point-occupancy tests
- support-candidate tests
- line-of-sight tests
- collision domain tests
- visibility domain tests
- navigation-domain tests where supported

Guide-level example:

```wr
test landing_replaces_base_band() {
    seed_band(32, 44_u64)
    EncounterDirector.state = foldmother_fixture_state(band=32)

    world = capture StaircaseWorld
    ray = Ray3(
        origin=vec3(0.0, 6.0, 0.0),
        direction=normalize(vec3(0.0, -1.0, 0.0))
    )
    hit = nearest(domain=Combat(world=world), ray=ray, max_distance=40.0)

    expect hit.hit
    expect exact(hit.payload.material_id, landing_material_id())
}
```

### Domain Testing

Each `domain` MUST be testable as a contract, not just as a runtime configuration blob.

The implementation MUST support tests that validate:

- which detail level a domain selects
- whether material participation is enabled
- whether radiance participation is enabled
- whether media participation is enabled
- whether domain-specific error budgets are respected
- whether a declaration is legal in the target domain

This is important because a major source of bugs in the proposed language is not just wrong field math, but wrong domain routing.

### Move And Moveset Testing

`move` and `moveset` are first-class authored constructs and MUST be first-class test surfaces.

The implementation MUST support deterministic move simulation with:

- fixed timestep
- fixed duration
- explicit initial player/blade/target state
- stable contact reporting
- stable parry window behavior
- stable move transition behavior

Tests MUST be able to assert:

- contact counts and contact types
- move phase entry and exit timing
- position and orientation constraints
- allowed or rejected moveset transitions
- solver stability under fixed inputs

### Streaming And Residency Testing

Streaming is part of authored world semantics in this RFC and MUST therefore be testable.

The implementation MUST support tests that validate:

- region selection from topology plus follow target
- correct residency windows for `RegionLine`, `RegionGrid`, and `RegionGraph`
- correct sparse encounter insertion
- correct transition-region residency at biome or encounter boundaries
- descriptor eviction and insertion without identity corruption

For `Staircase`, it MUST be possible to test that only the expected nearby bands are resident for a given player band and follow radius.

### CPU Render And Probe Testing

The presentation stack MUST be testable without requiring live GPU inspection.

The implementation MUST support:

- CPU render or probe execution for `render`
- deterministic camera setup
- deterministic lighting setup
- deterministic media evaluation
- image capture or structured probe output

The CPU presentation backend MAY be slower than the GPU backend, but it MUST be correct enough to serve as a reference for render-oriented tests.

Probe-style testing SHOULD also be supported for lightweight assertions on:

- surface values
- radiance values
- media values
- visibility values
- post-light accumulation values

### CPU/GPU Differential Testing

The implementation MUST provide first-class CPU/GPU differential testing.

This capability MUST NOT require users to manually write separate shader harnesses.

Required comparison modes:

- differential point samples
- differential domain queries
- differential nearest-hit results
- differential material evaluation
- differential render or probe comparison

Guide-level example:

```wr
test gpu_matches_cpu_for_foldmother_collision() {
    world = staircase_boss_fixture(kind=BossKind.FoldMother, band=32)

    compare cpu and gpu query Combat(world=world) {
        rays = landing_probe_rays()
        payload_exact = true
        distance_abs = 0.0005
        normal_abs = 0.002
    }
}
```

The implementation MAY choose different exact syntax, but it MUST provide a built-in mechanism of equivalent power.

### Render Differential And Golden Testing

The implementation MUST support render-oriented regression testing.

Required capabilities:

- render a named `render` declaration deterministically
- compare against a golden reference image or probe baseline
- produce a difference artifact on failure
- compare CPU and GPU render results within configurable tolerances

Golden images or probe baselines used for tests are test fixtures, not authored gameplay content, and are therefore allowed by this RFC.

Guide-level example:

```wr
test staircase_view_matches_reference() {
    world = staircase_boss_fixture(kind=BossKind.FoldMother, band=32)
    image = render_frame(
        view=StaircaseView(
            world=world,
            camera=test_camera_at_landing()
        )
    )

    expect image_matches(
        actual=image,
        golden="staircase_foldmother_landing",
        max_rms=0.01,
        max_channel=0.03
    )
}
```

### Filtering And Derivative Semantics In Tests

Procedural materials and effects MUST have defined host-side semantics for filtering-sensitive operations.

The implementation MUST NOT require GPU-only hidden derivative behavior to make material code meaningful.

Therefore:

- filtered noise MUST have CPU semantics
- footprint-aware material functions MUST have CPU semantics
- CPU material tests MUST not depend on unavailable backend-only state

If a material function requires a sampling footprint, that footprint MUST be constructible in CPU tests.

### Test Runner Behavior

The implementation MUST provide a built-in test runner for the language.

At minimum, the runner MUST:

- discover `test` declarations
- execute each test in isolation
- surface deterministic seed/time context in output
- report assertion failures with value details
- report spatial query failures with query context
- report image failures with diff metadata
- support filtering and targeted execution

The guide-level command remains:

- `wrela test <path>`

The implementation MAY provide additional flags for:

- backend selection
- tolerance overrides
- image update workflows
- deterministic seed overrides
- parallel execution

### Failure Reporting And Diagnostics

The test system MUST produce high-signal failure output.

At minimum, failures SHOULD include:

- source test name and location
- failing assertion kind
- exact versus approximate comparison mode
- expected and actual values
- tolerance values when relevant
- domain name when relevant
- region/archetype/generator identity when relevant
- seed and clock values

Render failures SHOULD include:

- image dimensions
- aggregate diff summary
- worst offending pixel or tile
- path to generated diff artifact when supported

Query failures SHOULD include:

- ray origin and direction where applicable
- max distance where applicable
- nearest-hit summary
- payload summary

### Required Compiler And Runtime Support For Testability

The compiler MUST:

- emit CPU-callable entrypoints for portable declarations
- emit reflection metadata for detail levels, supports, payload schema, and domain participation
- preserve stable payload/layout rules across CPU and GPU backends
- make it possible for the test runner to invoke portable declarations without custom glue code

The runtime MUST:

- expose headless query execution
- expose headless render/probe execution
- support deterministic capture construction
- support deterministic move simulation
- support deterministic residency control

### Tooling Expectations For Great Developer Experience

To meet the devex bar motivating this RFC, the implementation SHOULD provide:

- inline preview of failing field samples
- interactive diff viewers for render failures
- CPU/GPU comparison summaries per domain
- quick rerun for one test with fixed seed
- move replay scrubbers for failed combat tests
- support and residency overlays for failed world tests

The intent is that GPU-heavy authored code should feel no harder to test than ordinary host code.

### Acceptance Expectations For Testing

An implementation has not satisfied this RFC unless:

- every portable declaration can be tested on CPU
- domain queries can be tested without launching the full game
- render behavior can be tested headlessly
- move/moveset behavior can be tested deterministically
- CPU/GPU differential testing is built in
- developers are not required to hand-author bespoke GPU test harnesses

## Staircase Reference Design

This section is normative for the completeness of the design target and informative for exact syntax.

### Core State

```wr
enum BossKind {
    none
    HingeSaint
    RibbonMaw
    BellArchivist
    FoldMother
    ChoirOfAngles
    NullColossus
    WitnessAtStepEnd
}

enum BossPhase {
    dormant
    intro
    phase_one
    phase_two
    phase_three
    dead
}

enum KatanaMoveId {
    idle
    orbit_guard
    draw_sever
    pinion_lunge
    tether_counter
}

value PlayerState {
    transform: Transform3
    velocity: Vec3
    band: I32
    ascent: F32
    stamina: F32
    focus: F32
}

value BladeState {
    transform: Transform3
    linear_velocity: Vec3
    angular_velocity: Vec3
    charge: F32
    move_id: KatanaMoveId
    move_time: F32
}

value TargetState {
    handle: ActorHandle
    center: Vec3
    velocity: Vec3
    radius: F32
    locked_on: Boolean
}

value EncounterState {
    kind: BossKind
    band: I32
    phase: BossPhase
    health: F32
    active: Boolean
    terrain_pressure: F32
    fold: F32
    seed: U64
}
```

### Materials And Effects

```wr
material staircase_stone(hit: Hit3, seed: U64) -> Surface { ... }
material katana_steel(hit: Hit3, blade: BladeState) -> Surface { ... }
radiance field staircase_sky(direction: Vec3, height: F32, time: F32) -> Vec3 { ... }
volume field abyss_haze(p: Vec3, time: F32) -> Medium { ... }
volume field katana_wake(p: Vec3, blade: BladeState) -> Medium { ... }
```

### Stair Band Generator

```wr
generator StairBand(key: BandKey, seed: U64) {
    support = staircase_band_support(key=key)

    detail coarse field exact distance(p: Vec3) -> F32 { ... }
    detail fine field conservative distance(p: Vec3) -> F32 { ... }
    material surface(hit: Hit3) -> Surface { ... }
}
```

### Boss Landing

```wr
generator BossLanding(kind: BossKind, encounter: EncounterState, seed: U64) {
    support = landing_support(encounter.band)

    detail coarse field conservative distance(p: Vec3) -> F32 { ... }
    detail fine field conservative distance(p: Vec3) -> F32 { ... }
    material surface(hit: Hit3) -> Surface { ... }
}
```

### Katana Body And Moves

```wr
body KatanaBody(instance: BladeState) {
    mass = 2.6
    inertia = blade_inertia(...)
    collision detail exact distance(p: Vec3) -> F32 { ... }
    material surface(hit: Hit3) -> Surface { ... }
}

move OrbitGuard(player: PlayerState, blade: BladeState, target: TargetState) { ... }
move DrawSever(player: PlayerState, blade: BladeState, target: TargetState) { ... }
move TetherCounter(player: PlayerState, blade: BladeState, target: TargetState) { ... }

moveset KatanaArts {
    idle = OrbitGuard
    on light_attack when Targeting.state.locked_on => DrawSever
    on guard => OrbitGuard
    on parry => TetherCounter
}
```

### Boss Archetype With Terrain Contribution

```wr
archetype FoldMother(instance: EncounterState) {
    coarse_support = ...
    tight_support = ...

    terrain detail coarse field conservative distance(p: Vec3) -> F32 { ... }
    geometry detail fine field conservative distance(p: Vec3) -> F32 { ... }
    material surface(hit: Hit3) -> Surface { ... }
    radiance field emission(p: Vec3) -> Vec3 { ... }
    payload = ...
}
```

### World Composition

```wr
region StairBandRegion(band: I32, seed: U64, encounter: EncounterState) {
    place stairs = StairBand(key=BandKey(index=band), seed=seed)

    if encounter.band == band {
        replace landing = BossLanding(kind=encounter.kind, encounter=encounter, seed=seed)
    }

    if encounter.active and encounter.band == band and encounter.kind == BossKind.FoldMother {
        overlay boss = FoldMother(instance=encounter)
    }
}

space StaircaseWorld {
    streamed bands: RegionLine[StairBandRegion] radius 6 follow Player.state.band
    dynamic katana: Singleton[BladeState] using KatanaBody
}

domain Combat(world: Capture[StaircaseWorld]) {
    geometry_detail = coarse
    material = false
    radiance = false
    media = false
}

domain Presentation(world: Capture[StaircaseWorld], camera: Camera) {
    geometry_detail = fine
    material = true
    radiance = true
    media = true
}

render StaircaseView(world: Capture[StaircaseWorld], camera: Camera) { ... }
```

### Host Systems

```wr
system update_encounter[stage=fixed, reads=[Player, Clock], writes=[EncounterDirector]]() -> Nothing { ... }
system drive_katana[stage=fixed, reads=[Player, Targeting, Intent], writes=[Katana]]() -> Nothing { ... }

fn frame_world() -> Capture[StaircaseWorld] {
    return capture StaircaseWorld
}
```

`Staircase` is considered the reference content target for this RFC. A language implementation that cannot express the above architecture without falling back to imported assets has not met the design target.

## Open Questions

The following are intentionally left for later RFCs, but implementations SHOULD leave room for them:

- portable skeletal/deformation language beyond the initial `deform`/body model
- save schema and generator-versioning model
- author-facing editor integrations
- multiplayer replication model for captured worlds and dynamic descriptors
- offline compiler-guided precomputation policies for derived execution artifacts

## Acceptance Criteria

This RFC is satisfied when `wrela` can express, type-check, lower, and execute a field-native game architecture with the following properties:

- all authored world content is field-based
- no authored mesh or texture assets are required
- CPU and GPU consume one portable source language
- every portable declaration is directly testable on CPU
- built-in CPU/GPU differential testing exists for queries and renders
- per-domain backend emission allows production CPU stripping for presentation-only portable declarations
- the world is composed from streamed regions and dynamic archetypes
- capture epochs provide coherent immutable query snapshots with explicit retirement semantics
- composition conflicts, payload ownership, and overlap behavior are deterministic
- scatter and region generation are stable for unchanged explicit inputs
- move execution has one deterministic solver contract
- navigation domains define topology-aware semantics for impossible geometry
- materials, radiance, and volume effects are first-class
- physics-authored moves are first-class
- a standalone native binary or app bundle per target platform can be built with embedded GPU program artifacts and generated integration metadata
- a game with the structural needs of `Staircase` can be described coherently in the language

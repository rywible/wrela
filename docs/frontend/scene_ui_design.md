# Wrela Scene UI Design (2D + 3D + Backend-Integrated Optimism)

Status: Draft
Owner: Language + Runtime
Last updated: 2026-02-13

## 1. Purpose

Capture the current design direction for native frontend support in Wrela so we do not lose momentum or intent.

Core thesis:
- UI is a scene simulation problem, not a static DOM/layout problem.
- A page is a graph of nested scenes.
- Scenes should support 2D and 3D with one consistent model.
- Frontend optimistic updates must reuse backend domain logic directly.

## 2. Design Goals

1. Keep Wrela's existing strengths: deterministic behavior, explicit effects, strong typing, and compile-time guardrails.
2. Make UI syntax first-class and highly readable.
3. Split concerns clearly in one scene file: hierarchy, layout/style, and behavior.
4. Support 2D and 3D without introducing two unrelated paradigms.
5. Integrate natively with backend command/event domain logic for optimistic UI.

## 3. Non-Goals (for initial implementation)

1. Building a full gameplay/physics engine.
2. Implementing every rendering feature in v1 (advanced post-processing, full particle stack, etc.).
3. Replacing backend authority with client authority.

## 4. Scene Model

A scene is a simulation boundary.

- Scenes are nestable.
- Each scene has local state and local layout/transition resolution.
- Parent scenes can position child scenes, but cannot mutate child internals unless explicitly exposed.
- Cross-scene communication happens via typed events/contracts.

Page model:
- A page is a top-level scene containing nested child scenes.

## 5. Proposed Syntax Shape

Use one scene construct with explicit concern blocks.

```wr
scene landing_page(props: LandingProps):
    stage screen2d

    model:
        domain_adapter billing from BillingDomain via wasm("domain/billing.wasm")

    state:
        mutable selected_plan_id: String = ""
        mutable is_menu_open: Boolean = false

    tree:
        section hero:
            rectangle hero_backdrop
            text hero_title

        section pricing:
            repeat 3 as index:
                rectangle plan_card[index]
                text plan_label[index]

    layout:
        stack root vertical gap(64)
        size hero_backdrop(1200, 520)
        size plan_card[*](320, 420)
        distribute plan_card[*] horizontal gap(24)

    style:
        fill hero_backdrop(color.sky_050)
        fill plan_card[*](color.white)
        radius plan_card[*](18)

    transition:
        on enter(plan_card[*]):
            from opacity(0), y(16)
            to opacity(1), y(0)
            using spring(stiffness=180, damping=22) stagger(60ms)

    logic:
        on click(plan_card[1]):
            optimistic billing.send(BillingCommand.SelectPlan(plan_id="pro"))

        on domain_event billing.PlanSelected(plan_id):
            selected_plan_id = plan_id
```

### 5.1 Why these blocks

- `tree`: hierarchy and object composition.
- `layout`: spatial constraints and placement rules.
- `style`: visual/material declarations.
- `state`: local mutable scene state.
- `logic`: events, intents, and state changes.
- `transition`: declarative motion over state/layout changes.
- `model`: backend/shared-domain bindings.

This keeps Svelte-like concern separation while preserving one canonical scene unit.

## 6. Shape Primitives

Use full names (no abbreviations):

2D primitives:
- `rectangle`
- `circle`
- `oval`
- `line`
- `polygon`
- `text`
- `image`

3D primitives:
- `box`
- `sphere`
- `mesh`

Rule:
- Abbreviations like `rect` are invalid syntax.

Rationale:
- Readability and intent clarity matter more than shorthand.

## 7. 2D and 3D Unification

Use `stage` to define coordinate/render mode, while keeping the same scene semantics.

- `stage screen2d`
- `stage world3d`

### 7.1 2D example

```wr
scene pricing_2d(props: PricingProps):
    stage screen2d

    state:
        mutable selected_index: Integer = 1

    tree:
        row plans:
            repeat 3 as index:
                rectangle plan_card[index]
                text plan_label[index]

    layout:
        distribute plan_card[*] horizontal gap(24)

    style:
        fill plan_card[*](color.white)
        radius plan_card[*](18)

    transition:
        on change(selected_index):
            target plan_card[*]
            using spring(stiffness=220, damping=20)

    logic:
        on click(plan_card[0]): selected_index = 0
        on click(plan_card[1]): selected_index = 1
        on click(plan_card[2]): selected_index = 2
```

### 7.2 3D example

```wr
scene product_showcase_3d(props: ShowcaseProps):
    stage world3d

    camera main:
        projection perspective(fov=52deg, near=0.1, far=300)
        at (0, 1.2, 7) look_at (0, 0.8, 0)

    state:
        mutable orbit_speed: Number = 0.8

    tree:
        box product_body
        repeat 6 as index:
            sphere metric_orb[index]

    layout:
        orbit metric_orb[*] around product_body radius(2.6) axis(y)

    style:
        material product_body(material.glass)
        material metric_orb[*](material.sky_glow)

    transition:
        on enter(product_body):
            from rotation_y(-20deg), z(-0.8)
            to rotation_y(0deg), z(0)
            using spring(stiffness=140, damping=18)

    logic:
        on tick(dt_ms):
            spin metric_orb[*] y(speed=orbit_speed * dt_ms * 0.001)
```

## 8. Transition Semantics

`transition` is first-class and declarative.

Responsibilities:
- Define how visual state interpolates after `logic` changes.
- Support enter/exit/change/interaction transitions.
- Provide deterministic scheduling and ordering.

Determinism constraints:
- Transition resolution order must be stable.
- When conflicts exist, explicit policy applies (`swap`, `push_vertical`, `separate`, etc.).

## 9. Backend Integration (Native Optimistic Updates)

Frontend should not duplicate domain logic.

Approach:
1. Domain modules are written once in backend style.
2. Same modules compile to:
   - native backend target
   - WASM domain target for client prediction
3. Scene `model` binds to a domain adapter backed by WASM.
4. `logic` issues typed commands via `optimistic ...send(...)`.
5. Runtime reconciles with backend authority using ack/reject protocol.

### 9.1 Protocol sketch

Client send envelope:
- `command_id`
- `base_version`
- typed `command`

Server response:
- `accepted(events, new_version)`
- or `rejected(error, authoritative_state_version)`

Client behavior:
- on accepted: commit optimistic prediction.
- on rejected: rollback predicted events, apply authoritative state, replay pending commands.

## 10. Purity and Safety Rules

To preserve backend/frontend consistency:

1. Domain logic used for optimistic prediction must be pure and deterministic.
2. Domain layer cannot perform IO, timing, randomness, or host-dependent reads.
3. Side effects remain in application/integration layers, not domain reducers.
4. Scene checks remain pure (`check ... given ...`) and cannot mutate/await/spawn.

## 11. Accessibility Contract (First-Class)

Accessibility is a core platform requirement, not a post-hoc concern.

1. Scene `tree` must compile into a semantic accessibility tree.
2. Interactive nodes must be keyboard-focusable and keyboard-operable.
3. Interactive controls must provide an accessible name and role.
4. Focus order must be deterministic and visible.
5. Transition/motion must honor reduced-motion preferences.
6. Critical dynamic updates must support assistive announcements.
7. 3D interactions must provide an accessible 2D/semantic fallback path.

Compiler/runtime enforcement direction:
- emit diagnostics for missing roles/labels on interactive elements
- emit diagnostics for focus traps and unreachable controls
- support contrast/touch-target lint checks in style/layout passes
- auto-wire reduced-motion behavior in transition system

## 12. Compilation Strategy (Target View)

Compiler pipeline (conceptual):
1. Parse scene blocks into Scene AST.
2. Lower to typed Scene IR (tree/layout/style/transition/logic).
3. Lower domain modules to both native and WASM targets.
4. Generate runtime bindings for command/event reconciliation.
5. Web backend emits WebGPU renderer path (with WebGL2/Canvas fallback later).

## 13. Suggested Phasing

### Phase 0: Contract freeze (now)
- Finalize syntax and semantics in docs/spec draft.
- Freeze domain WASM ABI and optimistic protocol.

### Phase 1: Backend transport foundation
- Complete HTTP + WebSocket + auth/session + command/event envelope.
- Ensure versioned authoritative responses for optimistic reconciliation.

### Phase 2: Language + compiler skeleton
- Parse and typecheck scene blocks.
- Emit no-op/diagnostic runtime stubs.
- Compile pure domain modules to WASM target.

### Phase 3: Runtime MVP (2D)
- Implement 2D primitives + layout + transition engine.
- Integrate optimistic command flow with backend reconciliation.

### Phase 4: 3D extension
- Add `world3d`, camera, 3D primitives, and spatial layout/resolution.
- Support mixed 2D overlays in 3D scenes.

### Phase 5: Production hardening
- Performance profiling and batching.
- Rendering fallback strategy.
- Tooling, diagnostics, and test harnesses.

## 14. Open Questions

1. Exact grammar details for `layout`, `style`, and `transition` sub-statements.
2. Final conflict policy vocabulary and default behavior.
3. Text rendering pipeline details for WebGPU target.
4. Hot reload and editor experience expectations.
5. Naming-rule adaptations for scene-specific identifiers, if any.
6. Exact accessibility syntax shape (`a11y` block vs node-level annotations).

## 15. Immediate Next Steps

1. Add a formal grammar draft section in `language/spec/spec.wr` comments or companion doc.
2. Define WASM ABI for domain prediction (`decide/evolve` exports and data encoding).
3. Define optimistic transport envelope in backend contracts docs.
4. Create parser spike for `scene` block with concern sections.

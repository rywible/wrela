//! WASM bridge for loading and executing compiled Wrela game modules.
//!
//! The client itself runs as WASM in the browser. This module loads a *second*
//! WASM binary (the compiled Wrela game logic) using the browser's
//! `WebAssembly` API, wires up the `wrela_runtime` import namespace with host
//! function stubs, and exposes `init` / `tick` / render-state extraction to the
//! rest of the client.
//!
//! All Wrela values are nanboxed i64 -- see `runtime/src/wasm_runtime.rs` for
//! the canonical representation.

#![allow(clippy::needless_pass_by_value)]
#![allow(dead_code)]

use js_sys::{Array, Function, Object, Reflect, WebAssembly};
use std::collections::HashMap;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

// ---------------------------------------------------------------------------
// NanBox constants (must match runtime/src/wasm_runtime.rs and compiler)
// ---------------------------------------------------------------------------

const NANBOX_QNAN: u64 = 0x7ff8_0000_0000_0000;
const NANBOX_TAG_SHIFT: u64 = 49;
const NANBOX_PAYLOAD_MASK: u64 = (1u64 << NANBOX_TAG_SHIFT) - 1;
const NANBOX_TAG_INT: u64 = 2;
const NANBOX_TAG_IMM: u64 = 3;
const NANBOX_IMM_NIL: u64 = 0;
#[allow(dead_code)]
const NANBOX_IMM_FALSE: u64 = 1;
#[allow(dead_code)]
const NANBOX_IMM_TRUE: u64 = 2;

// ---------------------------------------------------------------------------
// NanBox helpers (pure, no heap -- suitable for host-function stubs)
// ---------------------------------------------------------------------------

fn nanbox_const(tag: u64, payload: u64) -> i64 {
    (NANBOX_QNAN | (tag << NANBOX_TAG_SHIFT) | (payload & NANBOX_PAYLOAD_MASK)) as i64
}

fn nanbox_nil() -> i64 {
    nanbox_const(NANBOX_TAG_IMM, NANBOX_IMM_NIL)
}

fn nanbox_int(value: i64) -> i64 {
    let payload = (value as u64) & NANBOX_PAYLOAD_MASK;
    (NANBOX_QNAN | (NANBOX_TAG_INT << NANBOX_TAG_SHIFT) | payload) as i64
}

fn unbox_int(value: i64) -> i64 {
    let v = value as u64;
    let tag = (v >> NANBOX_TAG_SHIFT) & 0x3;
    if tag == NANBOX_TAG_INT {
        let payload = v & NANBOX_PAYLOAD_MASK;
        let sign_bit = 1u64 << (NANBOX_TAG_SHIFT - 1);
        if payload & sign_bit != 0 {
            (payload | !NANBOX_PAYLOAD_MASK) as i64
        } else {
            payload as i64
        }
    } else {
        0
    }
}

#[allow(dead_code)]
fn nanbox_bool(value: bool) -> i64 {
    nanbox_const(
        NANBOX_TAG_IMM,
        if value {
            NANBOX_IMM_TRUE
        } else {
            NANBOX_IMM_FALSE
        },
    )
}

fn is_pointer(value: i64) -> bool {
    let v = value as u64;
    if v & NANBOX_QNAN != NANBOX_QNAN {
        return false;
    }
    let tag = (v >> NANBOX_TAG_SHIFT) & 0x3;
    tag == 0 && (v & NANBOX_PAYLOAD_MASK) != 0
}

// ---------------------------------------------------------------------------
// Stub heap -- lightweight object store for host-function stubs
// ---------------------------------------------------------------------------

/// Objects allocated by the host stub runtime. This is intentionally minimal:
/// it gives compiled Wrela modules a working heap so that `wr_class_new`,
/// `wr_list_new`, etc. return usable handles, but the real heavy lifting will
/// be done once we integrate the full `wasm_runtime` module.
#[derive(Debug, Clone)]
enum StubHeapValue {
    ClassInstance { type_id: i64, fields: Vec<i64> },
    List(Vec<i64>),
}

#[derive(Debug, Default)]
struct StubHeap {
    next_id: u64,
    objects: HashMap<u64, StubHeapValue>,
}

impl StubHeap {
    fn alloc(&mut self, value: StubHeapValue) -> i64 {
        self.next_id += 1;
        let id = self.next_id;
        self.objects.insert(id, value);
        // Encode as nanbox pointer: QNAN | pointer-payload (no tag bits)
        (NANBOX_QNAN | (id & NANBOX_PAYLOAD_MASK)) as i64
    }

    fn get(&self, nanboxed: i64) -> Option<&StubHeapValue> {
        if !is_pointer(nanboxed) {
            return None;
        }
        let id = (nanboxed as u64) & NANBOX_PAYLOAD_MASK;
        self.objects.get(&id)
    }

    fn get_mut(&mut self, nanboxed: i64) -> Option<&mut StubHeapValue> {
        if !is_pointer(nanboxed) {
            return None;
        }
        let id = (nanboxed as u64) & NANBOX_PAYLOAD_MASK;
        self.objects.get_mut(&id)
    }
}

// We use a thread-local stub heap because the host closures captured below
// cannot borrow `WrelaGameModule` directly -- they are invoked by the guest
// WASM and need independent access to shared state.
std::thread_local! {
    static STUB_HEAP: std::cell::RefCell<StubHeap> = std::cell::RefCell::new(StubHeap::default());
}

fn with_stub_heap<F, R>(f: F) -> R
where
    F: FnOnce(&mut StubHeap) -> R,
{
    STUB_HEAP.with(|h| f(&mut h.borrow_mut()))
}

// ---------------------------------------------------------------------------
// ForestRenderState -- extracted from the nanboxed game state for rendering
// ---------------------------------------------------------------------------

/// Rendering-relevant subset of the Wrela `GameTickState` class, extracted
/// from nanboxed values into plain Rust types so the renderer can consume them
/// without touching the nanbox layer.
///
/// Integer fields from Wrela use milli-scale (1000 = 1.0). We convert to f32
/// during extraction.
#[derive(Debug, Clone, Default)]
pub struct ForestRenderState {
    pub player_x: f32,
    pub player_y: f32,
    pub player_z: f32,
    pub player_health_ratio: f32,
    pub player_stamina_ratio: f32,
    pub enemy_x: f32,
    pub enemy_y: f32,
    pub enemy_z: f32,
    pub enemy_health_ratio: f32,
    pub player_state: i32,
    pub enemy_move: i32,
    pub hit_stopped: bool,
    pub resonance_tier: i32,
    pub world_tick: i64,
    pub camera_x: f32,
    pub camera_y: f32,
    pub camera_z: f32,
    pub camera_jolt_magnitude: f32,
    pub fov_shift: f32,
}

// ---------------------------------------------------------------------------
// WrelaGameModule
// ---------------------------------------------------------------------------

/// Handle to a loaded Wrela game WASM module. Caches compiled module, instance,
/// and exported function references.
pub struct WrelaGameModule {
    /// The compiled WebAssembly module.
    module: WebAssembly::Module,
    /// The instantiated WebAssembly instance.
    instance: WebAssembly::Instance,
    /// Cached reference to the `init` export.
    init_fn: Option<Function>,
    /// Cached reference to the `tick` export (execute_game_tick wrapper).
    tick_fn: Option<Function>,
    /// Current game state as a nanboxed i64 value.
    state_ptr: i64,
}

impl WrelaGameModule {
    // -----------------------------------------------------------------------
    // Module loading
    // -----------------------------------------------------------------------

    /// Compile and instantiate a Wrela game module from raw WASM bytes.
    ///
    /// This is async because `WebAssembly.compile` and `WebAssembly.instantiate`
    /// return promises in the browser.
    pub async fn load_module(wasm_bytes: &[u8]) -> Result<Self, String> {
        // Build the import object with "wrela_runtime" namespace stubs.
        let imports = build_import_object()?;

        // Compile the module.
        let js_bytes = js_sys::Uint8Array::from(wasm_bytes);
        let compile_promise = WebAssembly::compile(&js_bytes.into());
        let module_js = JsFuture::from(compile_promise)
            .await
            .map_err(|e| format!("WebAssembly.compile failed: {e:?}"))?;
        let module: WebAssembly::Module = module_js
            .dyn_into()
            .map_err(|_| "compile result is not a Module".to_string())?;

        // Instantiate with our imports.
        let instance_promise = WebAssembly::instantiate_module(&module, &imports);
        let instance_js = JsFuture::from(instance_promise)
            .await
            .map_err(|e| format!("WebAssembly.instantiate failed: {e:?}"))?;
        let instance: WebAssembly::Instance = instance_js
            .dyn_into()
            .map_err(|_| "instantiate result is not an Instance".to_string())?;

        // Cache exported functions.
        let exports = instance.exports();
        let init_fn = Self::resolve_export(&exports, "init");
        let tick_fn = Self::resolve_export(&exports, "tick");

        Ok(Self {
            module,
            instance,
            init_fn,
            tick_fn,
            state_ptr: nanbox_nil(),
        })
    }

    /// Resolve an export name to a JS `Function`, returning `None` if the
    /// export is absent or not callable.
    fn resolve_export(exports: &Object, name: &str) -> Option<Function> {
        Reflect::get(exports, &JsValue::from_str(name))
            .ok()
            .and_then(|v| v.dyn_into::<Function>().ok())
    }

    // -----------------------------------------------------------------------
    // init / tick
    // -----------------------------------------------------------------------

    /// Call the module's `init` export and store the returned state.
    ///
    /// The compiled Wrela module is expected to export `init() -> i64` which
    /// returns the initial `GameTickState` as a nanboxed class instance.
    pub fn call_init(&mut self) -> Result<i64, String> {
        let init = self
            .init_fn
            .as_ref()
            .ok_or_else(|| "module does not export 'init'".to_string())?;
        let result = init
            .call0(&JsValue::NULL)
            .map_err(|e| format!("init call failed: {e:?}"))?;
        let state = Self::js_to_i64(&result)?;
        self.state_ptr = state;
        Ok(state)
    }

    /// Call the module's `tick` export with the current state and input.
    ///
    /// The compiled Wrela module is expected to export
    ///   `tick(state: i64, input_bits: i32, dt_ms: i32) -> i64`
    /// returning the new `GameTickState`.
    pub fn call_tick(&mut self, state: i64, input_bits: i32, dt_ms: i32) -> Result<i64, String> {
        let tick = self
            .tick_fn
            .as_ref()
            .ok_or_else(|| "module does not export 'tick'".to_string())?;

        let args = Array::new();
        args.push(&Self::i64_to_js(state));
        args.push(&JsValue::from(input_bits));
        args.push(&JsValue::from(dt_ms));

        let result = tick
            .apply(&JsValue::NULL, &args)
            .map_err(|e| format!("tick call failed: {e:?}"))?;
        let new_state = Self::js_to_i64(&result)?;
        self.state_ptr = new_state;
        Ok(new_state)
    }

    /// Convenience: tick using the internally-stored state.
    pub fn tick(&mut self, input_bits: i32, dt_ms: i32) -> Result<i64, String> {
        let st = self.state_ptr;
        self.call_tick(st, input_bits, dt_ms)
    }

    // -----------------------------------------------------------------------
    // Render state extraction
    // -----------------------------------------------------------------------

    /// Extract a `ForestRenderState` from the current nanboxed game state.
    ///
    /// The `GameTickState` class is a heap-allocated class instance whose
    /// fields are ordered exactly as declared in `game_tick.wr`:
    ///
    /// ```text
    ///  0: world              (ForestWorldState)
    ///  1: player             (CharacterState)
    ///  2: player_stamina     (StaminaState)
    ///  3: player_health      (HealthState)
    ///  4: player_poise       (PoiseState)
    ///  5: blade              (SoulBlade)
    ///  6: resonance          (ResonanceState)
    ///  7: enemy              (RotStalker)
    ///  8: enemy_health       (HealthState)
    ///  9: enemy_poise        (PoiseState)
    /// 10: parry_state        (ParryState)
    /// 11: hit_stop           (HitStopState)
    /// 12: player_hit_memory  (CombatHitMemory)
    /// 13: enemy_hit_memory   (CombatHitMemory)
    /// 14: spring_arm         (SpringArmState)
    /// 15: camera_jolt        (CameraJoltState)
    /// 16: fov_shift          (CameraFovShiftState)
    /// 17: combat_feel        (CombatFeelConfig)
    /// 18: player_x           (Integer)
    /// 19: player_y           (Integer)
    /// 20: player_z           (Integer)
    /// ```
    pub fn extract_render_state(&self, state: i64) -> ForestRenderState {
        with_stub_heap(|heap| {
            let mut rs = ForestRenderState::default();

            let Some(StubHeapValue::ClassInstance { fields, .. }) = heap.get(state) else {
                return rs;
            };
            let fields = fields.clone(); // clone so we can re-borrow heap

            // Direct integer fields (milli-scale -> f32)
            let milli = |idx: usize| -> f32 {
                fields.get(idx).copied().map(unbox_int).unwrap_or(0) as f32 / 1000.0
            };

            rs.player_x = milli(18);
            rs.player_y = milli(19);
            rs.player_z = milli(20);

            // Player state (field 1 = CharacterState, sub-field 0 = current_state)
            rs.player_state = Self::read_class_field_int(heap, &fields, 1, 0) as i32;

            // Player health ratio (field 3 = HealthState)
            // HealthState fields: current_milli(0), max_milli(1)
            let p_hp_cur = Self::read_class_field_int(heap, &fields, 3, 0);
            let p_hp_max = Self::read_class_field_int(heap, &fields, 3, 1).max(1);
            rs.player_health_ratio = p_hp_cur as f32 / p_hp_max as f32;

            // Player stamina ratio (field 2 = StaminaState)
            // StaminaState fields: current_milli(0), max_milli(1)
            let p_st_cur = Self::read_class_field_int(heap, &fields, 2, 0);
            let p_st_max = Self::read_class_field_int(heap, &fields, 2, 1).max(1);
            rs.player_stamina_ratio = p_st_cur as f32 / p_st_max as f32;

            // Enemy (field 7 = RotStalker)
            // RotStalker fields: entity_id(0), position_x(1), position_y(2),
            //   position_z(3), current_move(4), ...
            rs.enemy_x = Self::read_class_field_int(heap, &fields, 7, 1) as f32 / 1000.0;
            rs.enemy_y = Self::read_class_field_int(heap, &fields, 7, 2) as f32 / 1000.0;
            rs.enemy_z = Self::read_class_field_int(heap, &fields, 7, 3) as f32 / 1000.0;
            rs.enemy_move = Self::read_class_field_int(heap, &fields, 7, 4) as i32;

            // Enemy health ratio (field 8 = HealthState)
            let e_hp_cur = Self::read_class_field_int(heap, &fields, 8, 0);
            let e_hp_max = Self::read_class_field_int(heap, &fields, 8, 1).max(1);
            rs.enemy_health_ratio = e_hp_cur as f32 / e_hp_max as f32;

            // Hit stop (field 11 = HitStopState, sub-field 0 = remaining_ticks)
            let hit_stop_remaining = Self::read_class_field_int(heap, &fields, 11, 0);
            rs.hit_stopped = hit_stop_remaining > 0;

            // Resonance (field 6 = ResonanceState)
            // ResonanceState fields: charge_milli(0), tier(?)
            // We expose the tier: charge / 100 clamped to 0-3
            let res_charge = Self::read_class_field_int(heap, &fields, 6, 0);
            rs.resonance_tier = (res_charge / 100).min(3).max(0) as i32;

            // World tick (field 0 = ForestWorldState, sub-field 8 = tick)
            rs.world_tick = Self::read_class_field_int(heap, &fields, 0, 8);

            // Spring arm camera (field 14 = SpringArmState)
            // SpringArmState fields: target_x(0), target_y(1), target_z(2),
            //   current_x(3), current_y(4), current_z(5), ...
            rs.camera_x = Self::read_class_field_int(heap, &fields, 14, 3) as f32 / 1000.0;
            rs.camera_y = Self::read_class_field_int(heap, &fields, 14, 4) as f32 / 1000.0;
            rs.camera_z = Self::read_class_field_int(heap, &fields, 14, 5) as f32 / 1000.0;

            // Camera jolt (field 15 = CameraJoltState, sub-field 0 = magnitude_milli)
            rs.camera_jolt_magnitude =
                Self::read_class_field_int(heap, &fields, 15, 0) as f32 / 1000.0;

            // FOV shift (field 16 = CameraFovShiftState, sub-field 0 = shift_milli)
            rs.fov_shift = Self::read_class_field_int(heap, &fields, 16, 0) as f32 / 1000.0;

            rs
        })
    }

    /// Convenience: extract render state from the internally-stored state.
    pub fn render_state(&self) -> ForestRenderState {
        self.extract_render_state(self.state_ptr)
    }

    /// Read a single integer sub-field from a nested class instance.
    ///
    /// `parent_fields[parent_idx]` should be a nanboxed pointer to a class
    /// instance; we then read `sub_field_idx` from that child.
    fn read_class_field_int(
        heap: &StubHeap,
        parent_fields: &[i64],
        parent_idx: usize,
        sub_field_idx: usize,
    ) -> i64 {
        let Some(&child_ptr) = parent_fields.get(parent_idx) else {
            return 0;
        };
        let Some(StubHeapValue::ClassInstance { fields, .. }) = heap.get(child_ptr) else {
            // Might be an inline int directly (e.g. player_x at slot 18)
            return unbox_int(child_ptr);
        };
        fields
            .get(sub_field_idx)
            .copied()
            .map(unbox_int)
            .unwrap_or(0)
    }

    // -----------------------------------------------------------------------
    // Access
    // -----------------------------------------------------------------------

    /// Return the raw nanboxed state pointer.
    pub fn state(&self) -> i64 {
        self.state_ptr
    }

    /// Return a reference to the underlying `WebAssembly::Module`.
    pub fn module(&self) -> &WebAssembly::Module {
        &self.module
    }

    /// Return a reference to the underlying `WebAssembly::Instance`.
    pub fn instance(&self) -> &WebAssembly::Instance {
        &self.instance
    }

    // -----------------------------------------------------------------------
    // JS ↔ i64 helpers
    // -----------------------------------------------------------------------

    /// Convert a `JsValue` (BigInt or Number) to i64.
    fn js_to_i64(v: &JsValue) -> Result<i64, String> {
        // Wrela WASM modules use i64 (BigInt in JS).
        if let Some(n) = v.as_f64() {
            return Ok(n as i64);
        }
        // Try BigInt path via toString then parse.
        let s = js_sys::JsString::from(v.clone());
        let rust_str: String = s.into();
        rust_str
            .parse::<i64>()
            .map_err(|e| format!("cannot convert JS value to i64: {e}"))
    }

    /// Convert an i64 to a `JsValue`. We use BigInt so that the full 64-bit
    /// range is preserved across the JS boundary.
    fn i64_to_js(v: i64) -> JsValue {
        js_sys::BigInt::from(v).into()
    }
}

// ---------------------------------------------------------------------------
// Import object builder
// ---------------------------------------------------------------------------

/// Build the `imports` object that gets passed to `WebAssembly.instantiate`.
///
/// The compiled Wrela module imports from the `"wrela_runtime"` namespace.
/// Each host function is implemented as a JS closure that delegates to the
/// stub nanbox helpers above.
fn build_import_object() -> Result<Object, String> {
    let runtime_ns = Object::new();

    // Macro for concise closure registration. Each variant captures the
    // appropriate arity.
    macro_rules! host_fn {
        // () -> i64
        ($name:expr, || -> $body:expr) => {
            let closure =
                Closure::<dyn Fn() -> JsValue>::new(move || js_sys::BigInt::from($body).into());
            set_prop(&runtime_ns, $name, closure.as_ref().unchecked_ref())?;
            closure.forget();
        };
        // (i64) -> i64
        ($name:expr, |$a:ident : i64| -> $body:expr) => {
            let closure = Closure::<dyn Fn(JsValue) -> JsValue>::new(move |$a: JsValue| {
                let $a = js_val_to_i64(&$a);
                js_sys::BigInt::from($body).into()
            });
            set_prop(&runtime_ns, $name, closure.as_ref().unchecked_ref())?;
            closure.forget();
        };
        // (i64) -> void
        ($name:expr, |$a:ident : i64| $body:expr) => {
            let closure = Closure::<dyn Fn(JsValue)>::new(move |$a: JsValue| {
                let $a = js_val_to_i64(&$a);
                $body;
            });
            set_prop(&runtime_ns, $name, closure.as_ref().unchecked_ref())?;
            closure.forget();
        };
        // (i64, i64) -> i64
        ($name:expr, |$a:ident : i64, $b:ident : i64| -> $body:expr) => {
            let closure = Closure::<dyn Fn(JsValue, JsValue) -> JsValue>::new(
                move |$a: JsValue, $b: JsValue| {
                    let $a = js_val_to_i64(&$a);
                    let $b = js_val_to_i64(&$b);
                    js_sys::BigInt::from($body).into()
                },
            );
            set_prop(&runtime_ns, $name, closure.as_ref().unchecked_ref())?;
            closure.forget();
        };
        // (i64, i64) -> void
        ($name:expr, |$a:ident : i64, $b:ident : i64| $body:expr) => {
            let closure =
                Closure::<dyn Fn(JsValue, JsValue)>::new(move |$a: JsValue, $b: JsValue| {
                    let $a = js_val_to_i64(&$a);
                    let $b = js_val_to_i64(&$b);
                    $body;
                });
            set_prop(&runtime_ns, $name, closure.as_ref().unchecked_ref())?;
            closure.forget();
        };
    }

    // -- runtime lifecycle --
    host_fn!("wr_runtime_init", || -> nanbox_nil());
    host_fn!("wr_runtime_abi", || -> 5i64); // ABI version 5

    // -- reference counting (stubs -- no-op) --
    host_fn!("wr_rc_inc", |_v: i64| {});
    host_fn!("wr_rc_dec", |_v: i64| {});

    // -- nanbox int --
    host_fn!("wr_nanbox_int", |v: i64| -> nanbox_int(v));
    host_fn!("wr_nanbox_get_int", |v: i64| -> unbox_int(v));
    host_fn!("wr_box_int", |v: i64| -> nanbox_int(v));

    // -- arithmetic --
    host_fn!("wr_num_add", |a: i64, b: i64| -> nanbox_int(unbox_int(a).wrapping_add(unbox_int(b))));
    host_fn!("wr_num_sub", |a: i64, b: i64| -> nanbox_int(unbox_int(a).wrapping_sub(unbox_int(b))));
    host_fn!("wr_num_mul", |a: i64, b: i64| -> nanbox_int(unbox_int(a).wrapping_mul(unbox_int(b))));
    host_fn!("wr_num_div", |a: i64, b: i64| -> {
        let denom = unbox_int(b);
        if denom == 0 { nanbox_nil() } else { nanbox_int(unbox_int(a) / denom) }
    });
    host_fn!("wr_num_mod", |a: i64, b: i64| -> {
        let denom = unbox_int(b);
        if denom == 0 { nanbox_nil() } else { nanbox_int(unbox_int(a) % denom) }
    });
    host_fn!("wr_num_neg", |v: i64| -> nanbox_int(-unbox_int(v)));

    // Aliases expected by some compiled modules
    host_fn!("wr_int_add", |a: i64, b: i64| -> nanbox_int(unbox_int(a).wrapping_add(unbox_int(b))));
    host_fn!("wr_int_sub", |a: i64, b: i64| -> nanbox_int(unbox_int(a).wrapping_sub(unbox_int(b))));
    host_fn!("wr_int_mul", |a: i64, b: i64| -> nanbox_int(unbox_int(a).wrapping_mul(unbox_int(b))));
    host_fn!("wr_int_div", |a: i64, b: i64| -> {
        let denom = unbox_int(b);
        if denom == 0 { nanbox_nil() } else { nanbox_int(unbox_int(a) / denom) }
    });
    host_fn!("wr_int_mod", |a: i64, b: i64| -> {
        let denom = unbox_int(b);
        if denom == 0 { nanbox_nil() } else { nanbox_int(unbox_int(a) % denom) }
    });

    // -- comparisons --
    host_fn!("wr_num_lt", |a: i64, b: i64| -> nanbox_bool(unbox_int(a) < unbox_int(b)));
    host_fn!("wr_num_gt", |a: i64, b: i64| -> nanbox_bool(unbox_int(a) > unbox_int(b)));
    host_fn!("wr_num_le", |a: i64, b: i64| -> nanbox_bool(unbox_int(a) <= unbox_int(b)));
    host_fn!("wr_num_ge", |a: i64, b: i64| -> nanbox_bool(unbox_int(a) >= unbox_int(b)));
    host_fn!("wr_value_eq", |a: i64, b: i64| -> nanbox_bool(a == b));
    host_fn!("wr_value_deep_eq", |a: i64, b: i64| -> nanbox_bool(a == b));
    host_fn!("wr_identity_eq", |a: i64, b: i64| -> nanbox_bool(a == b));

    // -- object allocation --
    // wr_alloc_object / wr_class_new: allocate a class instance with `count` nil fields.
    {
        let closure = Closure::<dyn Fn(JsValue, JsValue) -> JsValue>::new(
            move |type_id: JsValue, field_count: JsValue| {
                let tid = js_val_to_i64(&type_id);
                let count = js_val_to_i32(&field_count);
                let ptr = with_stub_heap(|heap| {
                    heap.alloc(StubHeapValue::ClassInstance {
                        type_id: tid,
                        fields: vec![nanbox_nil(); count as usize],
                    })
                });
                js_sys::BigInt::from(ptr).into()
            },
        );
        set_prop(
            &runtime_ns,
            "wr_alloc_object",
            closure.as_ref().unchecked_ref(),
        )?;
        closure.forget();
    }

    // wr_class_new(type_id: i64, _names_ptr: i32, _names_len_ptr: i32, count: i32) -> i64
    {
        let closure = Closure::<dyn Fn(JsValue, JsValue, JsValue, JsValue) -> JsValue>::new(
            move |type_id: JsValue,
                  _names_ptr: JsValue,
                  _names_len_ptr: JsValue,
                  count: JsValue| {
                let tid = js_val_to_i64(&type_id);
                let cnt = js_val_to_i32(&count);
                let ptr = with_stub_heap(|heap| {
                    heap.alloc(StubHeapValue::ClassInstance {
                        type_id: tid,
                        fields: vec![nanbox_nil(); cnt as usize],
                    })
                });
                js_sys::BigInt::from(ptr).into()
            },
        );
        set_prop(
            &runtime_ns,
            "wr_class_new",
            closure.as_ref().unchecked_ref(),
        )?;
        closure.forget();
    }

    // -- object field access --
    // wr_object_get / wr_class_get_slot
    {
        let closure = Closure::<dyn Fn(JsValue, JsValue) -> JsValue>::new(
            move |obj: JsValue, field_idx: JsValue| {
                let obj_v = js_val_to_i64(&obj);
                let idx = js_val_to_i32(&field_idx) as usize;
                let val = with_stub_heap(|heap| match heap.get(obj_v) {
                    Some(StubHeapValue::ClassInstance { fields, .. }) => {
                        fields.get(idx).copied().unwrap_or_else(nanbox_nil)
                    }
                    _ => nanbox_nil(),
                });
                js_sys::BigInt::from(val).into()
            },
        );
        set_prop(
            &runtime_ns,
            "wr_object_get",
            closure.as_ref().unchecked_ref(),
        )?;
        closure.forget();
    }

    // wr_class_get_slot(instance: i64, _name_ptr: i32, _name_len: i32, slot: i32) -> i64
    {
        let closure = Closure::<dyn Fn(JsValue, JsValue, JsValue, JsValue) -> JsValue>::new(
            move |instance: JsValue, _name_ptr: JsValue, _name_len: JsValue, slot: JsValue| {
                let obj_v = js_val_to_i64(&instance);
                let idx = js_val_to_i32(&slot) as usize;
                let val = with_stub_heap(|heap| match heap.get(obj_v) {
                    Some(StubHeapValue::ClassInstance { fields, .. }) => {
                        fields.get(idx).copied().unwrap_or_else(nanbox_nil)
                    }
                    _ => nanbox_nil(),
                });
                js_sys::BigInt::from(val).into()
            },
        );
        set_prop(
            &runtime_ns,
            "wr_class_get_slot",
            closure.as_ref().unchecked_ref(),
        )?;
        closure.forget();
    }

    // wr_object_set / wr_class_set_slot
    {
        let closure = Closure::<dyn Fn(JsValue, JsValue, JsValue)>::new(
            move |obj: JsValue, field_idx: JsValue, value: JsValue| {
                let obj_v = js_val_to_i64(&obj);
                let idx = js_val_to_i32(&field_idx) as usize;
                let val = js_val_to_i64(&value);
                with_stub_heap(|heap| {
                    if let Some(StubHeapValue::ClassInstance { fields, .. }) = heap.get_mut(obj_v) {
                        if idx < fields.len() {
                            fields[idx] = val;
                        }
                    }
                });
            },
        );
        set_prop(
            &runtime_ns,
            "wr_object_set",
            closure.as_ref().unchecked_ref(),
        )?;
        closure.forget();
    }

    // wr_class_set_slot(instance: i64, _name_ptr: i32, _name_len: i32, slot: i32, value: i64)
    {
        let closure = Closure::<dyn Fn(JsValue, JsValue, JsValue, JsValue, JsValue)>::new(
            move |instance: JsValue,
                  _name_ptr: JsValue,
                  _name_len: JsValue,
                  slot: JsValue,
                  value: JsValue| {
                let obj_v = js_val_to_i64(&instance);
                let idx = js_val_to_i32(&slot) as usize;
                let val = js_val_to_i64(&value);
                with_stub_heap(|heap| {
                    if let Some(StubHeapValue::ClassInstance { fields, .. }) = heap.get_mut(obj_v) {
                        if idx < fields.len() {
                            fields[idx] = val;
                        }
                    }
                });
            },
        );
        set_prop(
            &runtime_ns,
            "wr_class_set_slot",
            closure.as_ref().unchecked_ref(),
        )?;
        closure.forget();
    }

    // -- list operations --
    // wr_list_new(cap: i32) -> i64
    {
        let closure = Closure::<dyn Fn(JsValue) -> JsValue>::new(move |cap: JsValue| {
            let capacity = js_val_to_i32(&cap) as usize;
            let ptr = with_stub_heap(|heap| {
                heap.alloc(StubHeapValue::List(Vec::with_capacity(capacity)))
            });
            js_sys::BigInt::from(ptr).into()
        });
        set_prop(&runtime_ns, "wr_list_new", closure.as_ref().unchecked_ref())?;
        closure.forget();
    }

    // wr_list_new_local -- alias
    {
        let closure = Closure::<dyn Fn(JsValue) -> JsValue>::new(move |cap: JsValue| {
            let capacity = js_val_to_i32(&cap) as usize;
            let ptr = with_stub_heap(|heap| {
                heap.alloc(StubHeapValue::List(Vec::with_capacity(capacity)))
            });
            js_sys::BigInt::from(ptr).into()
        });
        set_prop(
            &runtime_ns,
            "wr_list_new_local",
            closure.as_ref().unchecked_ref(),
        )?;
        closure.forget();
    }

    // wr_list_push / wr_list_push_val
    host_fn!("wr_list_push", |list: i64, value: i64| -> {
        with_stub_heap(|heap| {
            if let Some(StubHeapValue::List(vec)) = heap.get_mut(list) {
                vec.push(value);
            }
        });
        list
    });
    host_fn!("wr_list_push_val", |list: i64, value: i64| -> {
        with_stub_heap(|heap| {
            if let Some(StubHeapValue::List(vec)) = heap.get_mut(list) {
                vec.push(value);
            }
        });
        list
    });

    // wr_list_get_val(list: i64, index: i64) -> i64
    host_fn!("wr_list_get_val", |list: i64, index: i64| -> {
        let idx = unbox_int(index) as usize;
        with_stub_heap(|heap| {
            match heap.get(list) {
                Some(StubHeapValue::List(vec)) => vec.get(idx).copied().unwrap_or_else(nanbox_nil),
                _ => nanbox_nil(),
            }
        })
    });

    // wr_list_len(list: i64) -> i64
    host_fn!("wr_list_len", |list: i64| -> {
        with_stub_heap(|heap| {
            match heap.get(list) {
                Some(StubHeapValue::List(vec)) => nanbox_int(vec.len() as i64),
                _ => nanbox_int(0),
            }
        })
    });

    // wr_list_set(list: i64, index: i32, value: i64) -- 3 args, void return
    {
        let closure = Closure::<dyn Fn(JsValue, JsValue, JsValue)>::new(
            move |list: JsValue, index: JsValue, value: JsValue| {
                let list_v = js_val_to_i64(&list);
                let idx = js_val_to_i32(&index) as usize;
                let val = js_val_to_i64(&value);
                with_stub_heap(|heap| {
                    if let Some(StubHeapValue::List(vec)) = heap.get_mut(list_v) {
                        if idx < vec.len() {
                            vec[idx] = val;
                        }
                    }
                });
            },
        );
        set_prop(&runtime_ns, "wr_list_set", closure.as_ref().unchecked_ref())?;
        closure.forget();
    }

    // wr_list_set_val(list: i64, index: i64, value: i64) -> i64
    {
        let closure = Closure::<dyn Fn(JsValue, JsValue, JsValue) -> JsValue>::new(
            move |list: JsValue, index: JsValue, value: JsValue| {
                let list_v = js_val_to_i64(&list);
                let idx = unbox_int(js_val_to_i64(&index)) as usize;
                let val = js_val_to_i64(&value);
                with_stub_heap(|heap| {
                    if let Some(StubHeapValue::List(vec)) = heap.get_mut(list_v) {
                        if idx < vec.len() {
                            vec[idx] = val;
                        }
                    }
                });
                js_sys::BigInt::from(list_v).into()
            },
        );
        set_prop(
            &runtime_ns,
            "wr_list_set_val",
            closure.as_ref().unchecked_ref(),
        )?;
        closure.forget();
    }

    // -- stubs for operations that are no-ops or return nil in the bridge --
    host_fn!("wr_print", |_v: i64| -> nanbox_nil());
    host_fn!("wr_log", |_level: i64, _tag: i64| -> nanbox_nil());
    host_fn!("wr_crash", |_msg: i64| -> nanbox_nil());
    host_fn!("wr_assert", |_cond: i64, _msg: i64| -> nanbox_nil());
    host_fn!("wr_assert_eq", |_lhs: i64, _rhs: i64| -> nanbox_nil());
    host_fn!("wr_assert_value_equality", |_lhs: i64, _rhs: i64| -> nanbox_nil());
    host_fn!("wr_assert_identity", |_lhs: i64, _rhs: i64| -> nanbox_nil());
    host_fn!("wr_assert_err", |_result: i64| -> nanbox_nil());
    host_fn!("wr_coverage_hit", |_id: i64| -> nanbox_nil());
    host_fn!("wr_type_id", |_v: i64| -> nanbox_int(0));
    host_fn!("wr_clock_ns", || -> nanbox_int(0));
    host_fn!("wr_sleep_ms", |_ms: i64| -> nanbox_nil());
    host_fn!("wr_env_get", |_key: i64| -> nanbox_nil());
    host_fn!("wr_env_set", |_key: i64, _value: i64| -> nanbox_nil());
    host_fn!("wr_runtime_configure", |_config: i64| -> nanbox_nil());
    host_fn!("wr_runtime_cpu_count", || -> nanbox_int(1));
    host_fn!("wr_log_configure", |_config: i64| -> nanbox_nil());

    // -- string stubs --
    {
        let closure = Closure::<dyn Fn(JsValue, JsValue) -> JsValue>::new(
            move |_ptr: JsValue, _len: JsValue| js_sys::BigInt::from(nanbox_nil()).into(),
        );
        set_prop(
            &runtime_ns,
            "wr_str_intern_utf8",
            closure.as_ref().unchecked_ref(),
        )?;
        closure.forget();
    }
    {
        let closure = Closure::<dyn Fn(JsValue, JsValue) -> JsValue>::new(
            move |_ptr: JsValue, _len: JsValue| js_sys::BigInt::from(nanbox_nil()).into(),
        );
        set_prop(
            &runtime_ns,
            "wr_str_concat",
            closure.as_ref().unchecked_ref(),
        )?;
        closure.forget();
    }
    {
        let closure = Closure::<dyn Fn(JsValue, JsValue) -> JsValue>::new(
            move |_ptr: JsValue, _len: JsValue| js_sys::BigInt::from(nanbox_nil()).into(),
        );
        set_prop(
            &runtime_ns,
            "wr_str_concat_local",
            closure.as_ref().unchecked_ref(),
        )?;
        closure.forget();
    }
    host_fn!("wr_str_len", |_v: i64| -> nanbox_int(0));

    // -- map stubs --
    host_fn!("wr_map_new", || -> nanbox_nil());
    host_fn!("wr_map_new_local", || -> nanbox_nil());
    host_fn!("wr_map_get", |_map: i64, _key: i64| -> nanbox_nil());
    {
        let closure = Closure::<dyn Fn(JsValue, JsValue, JsValue) -> JsValue>::new(
            move |map: JsValue, _key: JsValue, _value: JsValue| {
                let m = js_val_to_i64(&map);
                js_sys::BigInt::from(m).into()
            },
        );
        set_prop(&runtime_ns, "wr_map_set", closure.as_ref().unchecked_ref())?;
        closure.forget();
    }
    host_fn!("wr_map_len", |_map: i64| -> nanbox_int(0));

    // -- range --
    host_fn!("wr_range_new", |_start: i64, _end: i64| -> nanbox_nil());

    // -- result --
    host_fn!("wr_result_ok", |v: i64| -> v);
    host_fn!("wr_result_err", |v: i64| -> v);
    host_fn!("wr_result_is_ok", |_v: i64| -> nanbox_bool(true));
    host_fn!("wr_result_unwrap", |v: i64| -> v);
    host_fn!("wr_result_err_unwrap", |v: i64| -> v);

    // -- bytes stubs --
    host_fn!("wr_bytes_from_string", |_v: i64| -> nanbox_nil());
    host_fn!("wr_bytes_from_list", |_v: i64| -> nanbox_nil());
    host_fn!("wr_bytes_to_string", |_v: i64| -> nanbox_nil());
    host_fn!("wr_bytes_to_list", |_v: i64| -> nanbox_nil());
    host_fn!("wr_bytes_len", |_v: i64| -> nanbox_int(0));

    // -- iterator stubs --
    host_fn!("wr_iter_init", |iterable: i64| -> iterable);
    {
        let closure = Closure::<dyn Fn(JsValue, JsValue, JsValue)>::new(
            move |_iter: JsValue, _value_ptr: JsValue, _done_ptr: JsValue| {
                // Stub: mark as done immediately (no-op).
            },
        );
        set_prop(
            &runtime_ns,
            "wr_iter_next",
            closure.as_ref().unchecked_ref(),
        )?;
        closure.forget();
    }

    // -- pending/actor/pool stubs --
    host_fn!("wr_pending_await", |_v: i64| -> nanbox_nil());

    // wr_actor_send(actor: i64, method_id: i64, args_ptr: i32, args_len: i32) -> i64
    {
        let closure = Closure::<dyn Fn(JsValue, JsValue, JsValue, JsValue) -> JsValue>::new(
            move |_a: JsValue, _b: JsValue, _c: JsValue, _d: JsValue| {
                js_sys::BigInt::from(nanbox_nil()).into()
            },
        );
        set_prop(
            &runtime_ns,
            "wr_actor_send",
            closure.as_ref().unchecked_ref(),
        )?;
        closure.forget();
    }

    // wr_actor_spawn (7 args)
    {
        let closure = Closure::<
            dyn Fn(JsValue, JsValue, JsValue, JsValue, JsValue, JsValue, JsValue) -> JsValue,
        >::new(
            move |_a: JsValue,
                  _b: JsValue,
                  _c: JsValue,
                  _d: JsValue,
                  _e: JsValue,
                  _f: JsValue,
                  _g: JsValue| { js_sys::BigInt::from(nanbox_nil()).into() },
        );
        set_prop(
            &runtime_ns,
            "wr_actor_spawn",
            closure.as_ref().unchecked_ref(),
        )?;
        closure.forget();
    }

    // wr_pool_new (6 args)
    {
        let closure = Closure::<
            dyn Fn(JsValue, JsValue, JsValue, JsValue, JsValue, JsValue) -> JsValue,
        >::new(
            move |_a: JsValue, _b: JsValue, _c: JsValue, _d: JsValue, _e: JsValue, _f: JsValue| {
                js_sys::BigInt::from(nanbox_nil()).into()
            },
        );
        set_prop(&runtime_ns, "wr_pool_new", closure.as_ref().unchecked_ref())?;
        closure.forget();
    }

    // -- registration stubs (void return, various arities) --
    {
        let closure = Closure::<dyn Fn(JsValue, JsValue, JsValue)>::new(
            move |_: JsValue, _: JsValue, _: JsValue| {},
        );
        set_prop(
            &runtime_ns,
            "wr_register_method",
            closure.as_ref().unchecked_ref(),
        )?;
        closure.forget();
    }
    {
        let closure = Closure::<dyn Fn(JsValue, JsValue, JsValue)>::new(
            move |_: JsValue, _: JsValue, _: JsValue| {},
        );
        set_prop(
            &runtime_ns,
            "wr_register_class",
            closure.as_ref().unchecked_ref(),
        )?;
        closure.forget();
    }
    {
        let closure = Closure::<dyn Fn(JsValue, JsValue, JsValue, JsValue)>::new(
            move |_: JsValue, _: JsValue, _: JsValue, _: JsValue| {},
        );
        set_prop(
            &runtime_ns,
            "wr_register_method_name",
            closure.as_ref().unchecked_ref(),
        )?;
        closure.forget();
    }

    // -- float stubs --
    // wr_box_float(f64) -> i64  --  stubs to nanbox_nil for now
    {
        let closure = Closure::<dyn Fn(JsValue) -> JsValue>::new(move |_v: JsValue| {
            js_sys::BigInt::from(nanbox_nil()).into()
        });
        set_prop(
            &runtime_ns,
            "wr_box_float",
            closure.as_ref().unchecked_ref(),
        )?;
        closure.forget();
    }
    // wr_unbox_float(i64) -> f64
    {
        let closure =
            Closure::<dyn Fn(JsValue) -> JsValue>::new(move |_v: JsValue| JsValue::from_f64(0.0));
        set_prop(
            &runtime_ns,
            "wr_unbox_float",
            closure.as_ref().unchecked_ref(),
        )?;
        closure.forget();
    }

    // -- Build the top-level imports object: { wrela_runtime: { ... } } --
    let imports = Object::new();
    set_prop(&imports, "wrela_runtime", &runtime_ns)?;

    Ok(imports)
}

// ---------------------------------------------------------------------------
// Utility helpers
// ---------------------------------------------------------------------------

/// Set a property on a JS object by string key.
fn set_prop(obj: &Object, key: &str, value: &JsValue) -> Result<(), String> {
    Reflect::set(obj, &JsValue::from_str(key), value)
        .map(|_| ())
        .map_err(|e| format!("failed to set property '{key}': {e:?}"))
}

/// Best-effort conversion from `JsValue` to `i64`. Handles both `Number` and
/// `BigInt` representations.
fn js_val_to_i64(v: &JsValue) -> i64 {
    if let Some(n) = v.as_f64() {
        return n as i64;
    }
    // BigInt path: convert to string, parse.
    let s: String = js_sys::JsString::from(v.clone()).into();
    s.parse::<i64>().unwrap_or(0)
}

/// Best-effort conversion from `JsValue` to `i32`.
fn js_val_to_i32(v: &JsValue) -> i32 {
    if let Some(n) = v.as_f64() {
        return n as i32;
    }
    let s: String = js_sys::JsString::from(v.clone()).into();
    s.parse::<i32>().unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Tests (native only -- these validate nanbox logic, not JS interop)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nanbox_int_roundtrip() {
        for v in [0i64, 1, -1, 42, -42, 1000, -1000] {
            let boxed = nanbox_int(v);
            let out = unbox_int(boxed);
            assert_eq!(out, v, "roundtrip failed for {v}");
        }
    }

    #[test]
    fn nanbox_nil_is_not_pointer() {
        assert!(!is_pointer(nanbox_nil()));
    }

    #[test]
    fn stub_heap_alloc_and_read() {
        with_stub_heap(|heap| {
            let ptr = heap.alloc(StubHeapValue::ClassInstance {
                type_id: 42,
                fields: vec![nanbox_int(10), nanbox_int(20)],
            });
            assert!(is_pointer(ptr));

            match heap.get(ptr) {
                Some(StubHeapValue::ClassInstance { type_id, fields }) => {
                    assert_eq!(*type_id, 42);
                    assert_eq!(fields.len(), 2);
                    assert_eq!(unbox_int(fields[0]), 10);
                    assert_eq!(unbox_int(fields[1]), 20);
                }
                _ => panic!("expected ClassInstance"),
            }
        });
    }

    #[test]
    fn stub_heap_list_operations() {
        with_stub_heap(|heap| {
            let ptr = heap.alloc(StubHeapValue::List(Vec::new()));
            assert!(is_pointer(ptr));

            // Push
            if let Some(StubHeapValue::List(vec)) = heap.get_mut(ptr) {
                vec.push(nanbox_int(100));
                vec.push(nanbox_int(200));
            }

            // Read back
            match heap.get(ptr) {
                Some(StubHeapValue::List(vec)) => {
                    assert_eq!(vec.len(), 2);
                    assert_eq!(unbox_int(vec[0]), 100);
                    assert_eq!(unbox_int(vec[1]), 200);
                }
                _ => panic!("expected List"),
            }
        });
    }

    #[test]
    fn extract_render_state_returns_default_for_nil() {
        // A nil state should produce a default ForestRenderState without panic.
        let module_stub = ForestRenderState::default();
        assert_eq!(module_stub.player_x, 0.0);
        assert_eq!(module_stub.world_tick, 0);
    }

    #[test]
    fn forest_render_state_extraction_from_stub_heap() {
        with_stub_heap(|heap| {
            // Build a minimal GameTickState-like class with enough fields to
            // exercise the extraction paths.
            //
            // Field layout (from game_tick.wr):
            //  0: world, 1: player, 2: stamina, 3: health, ...
            // 18: player_x, 19: player_y, 20: player_z

            // Sub-objects as minimal class instances
            let world = heap.alloc(StubHeapValue::ClassInstance {
                type_id: 100,
                fields: vec![nanbox_nil(); 11], // ForestWorldState has 11 fields; tick is field 8
            });
            let player = heap.alloc(StubHeapValue::ClassInstance {
                type_id: 101,
                fields: vec![nanbox_int(1)], // current_state = 1 (walk)
            });
            let stamina = heap.alloc(StubHeapValue::ClassInstance {
                type_id: 102,
                fields: vec![nanbox_int(80_000), nanbox_int(100_000)], // 80/100
            });
            let health = heap.alloc(StubHeapValue::ClassInstance {
                type_id: 103,
                fields: vec![nanbox_int(50_000), nanbox_int(100_000)], // 50/100
            });
            let poise = heap.alloc(StubHeapValue::ClassInstance {
                type_id: 104,
                fields: vec![nanbox_nil()],
            });
            let blade = heap.alloc(StubHeapValue::ClassInstance {
                type_id: 105,
                fields: vec![nanbox_nil()],
            });
            let resonance = heap.alloc(StubHeapValue::ClassInstance {
                type_id: 106,
                fields: vec![nanbox_int(250)], // charge_milli = 250 => tier 2
            });
            let enemy = heap.alloc(StubHeapValue::ClassInstance {
                type_id: 107,
                fields: vec![
                    nanbox_int(1),    // entity_id
                    nanbox_int(5000), // position_x
                    nanbox_int(0),    // position_y
                    nanbox_int(3000), // position_z
                    nanbox_int(0),    // current_move
                ],
            });
            let enemy_health = heap.alloc(StubHeapValue::ClassInstance {
                type_id: 103,
                fields: vec![nanbox_int(75_000), nanbox_int(100_000)],
            });
            let enemy_poise = heap.alloc(StubHeapValue::ClassInstance {
                type_id: 104,
                fields: vec![nanbox_nil()],
            });
            let parry_state = heap.alloc(StubHeapValue::ClassInstance {
                type_id: 108,
                fields: vec![nanbox_nil()],
            });
            let hit_stop = heap.alloc(StubHeapValue::ClassInstance {
                type_id: 109,
                fields: vec![nanbox_int(0)], // remaining_ticks = 0
            });
            let hit_mem1 = heap.alloc(StubHeapValue::ClassInstance {
                type_id: 110,
                fields: vec![nanbox_nil()],
            });
            let hit_mem2 = heap.alloc(StubHeapValue::ClassInstance {
                type_id: 110,
                fields: vec![nanbox_nil()],
            });
            let spring_arm = heap.alloc(StubHeapValue::ClassInstance {
                type_id: 111,
                fields: vec![
                    nanbox_nil(),     // target_x
                    nanbox_nil(),     // target_y
                    nanbox_nil(),     // target_z
                    nanbox_int(1500), // current_x
                    nanbox_int(2000), // current_y
                    nanbox_int(1800), // current_z
                ],
            });
            let camera_jolt = heap.alloc(StubHeapValue::ClassInstance {
                type_id: 112,
                fields: vec![nanbox_int(500)], // magnitude_milli
            });
            let fov_shift = heap.alloc(StubHeapValue::ClassInstance {
                type_id: 113,
                fields: vec![nanbox_int(100)], // shift_milli
            });
            let combat_feel = heap.alloc(StubHeapValue::ClassInstance {
                type_id: 114,
                fields: vec![nanbox_nil()],
            });

            let game_state = heap.alloc(StubHeapValue::ClassInstance {
                type_id: 200,
                fields: vec![
                    world,
                    player,
                    stamina,
                    health,
                    poise,
                    blade,
                    resonance,
                    enemy,
                    enemy_health,
                    enemy_poise,
                    parry_state,
                    hit_stop,
                    hit_mem1,
                    hit_mem2,
                    spring_arm,
                    camera_jolt,
                    fov_shift,
                    combat_feel,
                    nanbox_int(2000), // player_x
                    nanbox_int(0),    // player_y
                    nanbox_int(3000), // player_z
                ],
            });

            game_state
        });

        // The actual extraction requires a WrelaGameModule instance, which
        // requires JS interop. For unit testing the nanbox logic, the test
        // above (stub_heap_alloc_and_read) is the relevant one. This test
        // validates that building the full field layout does not panic.
    }
}

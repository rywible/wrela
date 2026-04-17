use crate::data::list;
use crate::value::{TypeId, Value, int_value, type_id_raw};
use crate::{wr_rc_dec, wr_rc_inc};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GpuElementKind {
    Integer,
    Float,
    Vec(TypeId),
    Mat3,
    Mat4,
}

impl GpuElementKind {
    fn from_value(value: Value) -> Option<Self> {
        match type_id_raw(value) {
            x if x == TypeId::Integer as u32 => Some(Self::Integer),
            x if x == TypeId::Float as u32 => Some(Self::Float),
            x if x == TypeId::Vec2 as u32 => Some(Self::Vec(TypeId::Vec2)),
            x if x == TypeId::Vec3 as u32 => Some(Self::Vec(TypeId::Vec3)),
            x if x == TypeId::Vec4 as u32 => Some(Self::Vec(TypeId::Vec4)),
            x if x == TypeId::Quat as u32 => Some(Self::Vec(TypeId::Quat)),
            x if x == TypeId::Mat3 as u32 => Some(Self::Mat3),
            x if x == TypeId::Mat4 as u32 => Some(Self::Mat4),
            _ => None,
        }
    }

    fn matches(self, value: Value) -> bool {
        match self {
            Self::Integer => type_id_raw(value) == TypeId::Integer as u32,
            Self::Float => type_id_raw(value) == TypeId::Float as u32,
            Self::Vec(expected) => type_id_raw(value) == expected as u32,
            Self::Mat3 => type_id_raw(value) == TypeId::Mat3 as u32,
            Self::Mat4 => type_id_raw(value) == TypeId::Mat4 as u32,
        }
    }
}

#[derive(Clone)]
struct GpuBufferEntry {
    kind: GpuElementKind,
    data: Vec<Value>,
}

struct GpuBufferRegistry {
    next_handle: AtomicU64,
    buffers: Mutex<HashMap<u64, GpuBufferEntry>>,
}

impl GpuBufferRegistry {
    fn new() -> Self {
        Self {
            next_handle: AtomicU64::new(1),
            buffers: Mutex::new(HashMap::new()),
        }
    }

    fn insert(&self, entry: GpuBufferEntry) -> Option<i64> {
        let handle = next_positive_u64_handle(&self.next_handle)?;
        self.buffers.lock().ok()?.insert(handle, entry);
        Some(handle as i64)
    }

    fn with_buffer<R>(&self, handle: i64, f: impl FnOnce(&mut GpuBufferEntry) -> R) -> Option<R> {
        if handle <= 0 {
            return None;
        }
        let mut buffers = self.buffers.lock().ok()?;
        let buffer = buffers.get_mut(&(handle as u64))?;
        Some(f(buffer))
    }
}

fn buffer_registry() -> &'static GpuBufferRegistry {
    static REGISTRY: OnceLock<GpuBufferRegistry> = OnceLock::new();
    REGISTRY.get_or_init(GpuBufferRegistry::new)
}

struct GpuAtomicI32Registry {
    next_handle: AtomicU64,
    atomics: Mutex<HashMap<u64, std::sync::Arc<AtomicI32>>>,
}

impl GpuAtomicI32Registry {
    fn new() -> Self {
        Self {
            next_handle: AtomicU64::new(1),
            atomics: Mutex::new(HashMap::new()),
        }
    }

    fn insert(&self, value: i32) -> Option<i64> {
        let handle = next_positive_u64_handle(&self.next_handle)?;
        self.atomics
            .lock()
            .ok()?
            .insert(handle, std::sync::Arc::new(AtomicI32::new(value)));
        Some(handle as i64)
    }

    fn get(&self, handle: i64) -> Option<std::sync::Arc<AtomicI32>> {
        if handle <= 0 {
            return None;
        }
        self.atomics.lock().ok()?.get(&(handle as u64)).cloned()
    }

    fn remove(&self, handle: i64) -> bool {
        if handle <= 0 {
            return false;
        }
        let Ok(mut atomics) = self.atomics.lock() else {
            return false;
        };
        atomics.remove(&(handle as u64)).is_some()
    }
}

fn gpu_atomic_i32_registry() -> &'static GpuAtomicI32Registry {
    static REGISTRY: OnceLock<GpuAtomicI32Registry> = OnceLock::new();
    REGISTRY.get_or_init(GpuAtomicI32Registry::new)
}

struct GpuAtomicU32Registry {
    next_handle: AtomicU64,
    atomics: Mutex<HashMap<u64, std::sync::Arc<AtomicU32>>>,
}

impl GpuAtomicU32Registry {
    fn new() -> Self {
        Self {
            next_handle: AtomicU64::new(1),
            atomics: Mutex::new(HashMap::new()),
        }
    }

    fn insert(&self, value: u32) -> Option<i64> {
        let handle = next_positive_u64_handle(&self.next_handle)?;
        self.atomics
            .lock()
            .ok()?
            .insert(handle, std::sync::Arc::new(AtomicU32::new(value)));
        Some(handle as i64)
    }

    fn get(&self, handle: i64) -> Option<std::sync::Arc<AtomicU32>> {
        if handle <= 0 {
            return None;
        }
        self.atomics.lock().ok()?.get(&(handle as u64)).cloned()
    }

    fn remove(&self, handle: i64) -> bool {
        if handle <= 0 {
            return false;
        }
        let Ok(mut atomics) = self.atomics.lock() else {
            return false;
        };
        atomics.remove(&(handle as u64)).is_some()
    }
}

fn gpu_atomic_u32_registry() -> &'static GpuAtomicU32Registry {
    static REGISTRY: OnceLock<GpuAtomicU32Registry> = OnceLock::new();
    REGISTRY.get_or_init(GpuAtomicU32Registry::new)
}

fn next_positive_u64_handle(counter: &AtomicU64) -> Option<u64> {
    let max = i64::MAX as u64;
    loop {
        let current = counter.load(Ordering::Relaxed);
        if current == 0 || current > max {
            return None;
        }
        let next = if current == max { 0 } else { current + 1 };
        if counter
            .compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            return Some(current);
        }
    }
}

fn value_to_len(value: Value) -> Option<usize> {
    int_value(value)
        .filter(|value| *value >= 0)
        .and_then(|value| usize::try_from(value).ok())
}

fn value_to_u32(value: Value) -> u32 {
    int_value(value)
        .map(|value| value.max(0))
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0)
}

fn value_to_i32_strict(value: Value) -> Option<i32> {
    int_value(value).and_then(|value| i32::try_from(value).ok())
}

fn value_to_u32_strict(value: Value) -> Option<u32> {
    int_value(value).and_then(|value| u32::try_from(value).ok())
}

fn build_int_list3(values: [u32; 3]) -> Value {
    let out = list::list_new(3);
    list::list_set(out, 0, Value::from_int(values[0] as i64));
    list::list_set(out, 1, Value::from_int(values[1] as i64));
    list::list_set(out, 2, Value::from_int(values[2] as i64));
    out
}

fn encode_schedule(spec: DispatchScheduleSpec) -> Value {
    let (kind, param) = match spec {
        DispatchScheduleSpec::Deterministic => (0u64, 0u64),
        DispatchScheduleSpec::Reverse => (1u64, 0u64),
        DispatchScheduleSpec::Shuffle(seed) => (2u64, seed as u64),
        DispatchScheduleSpec::WorkgroupReverse => (3u64, 0u64),
        DispatchScheduleSpec::WorkgroupShuffle(seed) => (4u64, seed as u64),
        DispatchScheduleSpec::RoundRobinWorkgroups => (5u64, 0u64),
    };
    let encoded = (SCHEDULE_MAGIC << SCHEDULE_MAGIC_SHIFT)
        | (kind << SCHEDULE_KIND_SHIFT)
        | (param & SCHEDULE_PARAM_MASK);
    Value::from_int(encoded as i64)
}

fn decode_schedule(schedule: Value) -> Option<DispatchScheduleSpec> {
    let raw = u64::try_from(int_value(schedule)?).ok()?;
    if (raw >> SCHEDULE_MAGIC_SHIFT) != SCHEDULE_MAGIC {
        return None;
    }
    let kind = (raw >> SCHEDULE_KIND_SHIFT) & 0xff;
    let param = (raw & SCHEDULE_PARAM_MASK) as u32;
    match kind {
        0 => Some(DispatchScheduleSpec::Deterministic),
        1 => Some(DispatchScheduleSpec::Reverse),
        2 => Some(DispatchScheduleSpec::Shuffle(param)),
        3 => Some(DispatchScheduleSpec::WorkgroupReverse),
        4 => Some(DispatchScheduleSpec::WorkgroupShuffle(param)),
        5 => Some(DispatchScheduleSpec::RoundRobinWorkgroups),
        _ => None,
    }
}

const SCHEDULE_MAGIC: u64 = 0x57;
const SCHEDULE_MAGIC_SHIFT: u64 = 40;
const SCHEDULE_KIND_SHIFT: u64 = 32;
const SCHEDULE_PARAM_MASK: u64 = 0xffff_ffff;

#[derive(Clone, Default)]
enum ActiveDispatchSchedule {
    #[default]
    Deterministic,
    Reverse,
    Shuffle(Box<[u32]>),
    WorkgroupReverse,
    WorkgroupShuffle(Box<[u32]>),
    RoundRobinWorkgroups(Box<[u32]>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DispatchScheduleSpec {
    Deterministic,
    Reverse,
    Shuffle(u32),
    WorkgroupReverse,
    WorkgroupShuffle(u32),
    RoundRobinWorkgroups,
}

#[derive(Clone, Default)]
struct DispatchState {
    num_workgroups: [u32; 3],
    workgroup_size: [u32; 3],
    total_size: [u32; 3],
    total_count: usize,
    schedule: ActiveDispatchSchedule,
    workgroup_id: [u32; 3],
    local_id: [u32; 3],
}

thread_local! {
    static DISPATCH_STACK: RefCell<Vec<DispatchState>> = const { RefCell::new(Vec::new()) };
}

fn with_current_dispatch<R>(f: impl FnOnce(&DispatchState) -> R) -> Option<R> {
    DISPATCH_STACK.with(|stack| stack.borrow().last().map(f))
}

fn with_current_dispatch_mut(f: impl FnOnce(&mut DispatchState)) -> bool {
    DISPATCH_STACK.with(|stack| {
        if let Some(state) = stack.borrow_mut().last_mut() {
            f(state);
            true
        } else {
            false
        }
    })
}

fn global_id(state: &DispatchState) -> [u32; 3] {
    [
        state.workgroup_id[0]
            .saturating_mul(state.workgroup_size[0])
            .saturating_add(state.local_id[0]),
        state.workgroup_id[1]
            .saturating_mul(state.workgroup_size[1])
            .saturating_add(state.local_id[1]),
        state.workgroup_id[2]
            .saturating_mul(state.workgroup_size[2])
            .saturating_add(state.local_id[2]),
    ]
}

pub fn gpu_buffer_new(len: Value, default_value: Value) -> Value {
    let Some(len) = value_to_len(len) else {
        return Value::nil();
    };
    let Some(kind) = GpuElementKind::from_value(default_value) else {
        return Value::nil();
    };

    let mut data = Vec::with_capacity(len);
    for _ in 0..len {
        data.push(default_value);
        unsafe {
            wr_rc_inc(default_value);
        }
    }
    buffer_registry()
        .insert(GpuBufferEntry { kind, data })
        .map(Value::from_int)
        .unwrap_or_else(Value::nil)
}

pub fn gpu_buffer_len(handle: Value) -> Value {
    let Some(handle) = int_value(handle) else {
        return Value::nil();
    };
    let Some(len) = buffer_registry().with_buffer(handle, |entry| entry.data.len()) else {
        return Value::nil();
    };
    i64::try_from(len)
        .map(Value::from_int)
        .unwrap_or_else(|_| Value::nil())
}

pub fn gpu_buffer_get(handle: Value, index: Value) -> Value {
    let Some(handle) = int_value(handle) else {
        return Value::nil();
    };
    let Some(index) = value_to_len(index) else {
        return Value::nil();
    };
    let Some(value) = buffer_registry().with_buffer(handle, |entry| entry.data.get(index).copied())
    else {
        return Value::nil();
    };
    let Some(value) = value else {
        return Value::nil();
    };
    unsafe {
        wr_rc_inc(value);
    }
    value
}

pub fn gpu_buffer_set(handle: Value, index: Value, value: Value) -> Value {
    let Some(handle) = int_value(handle) else {
        return Value::nil();
    };
    let Some(index) = value_to_len(index) else {
        return Value::nil();
    };
    let Some(old_value) = buffer_registry().with_buffer(handle, |entry| {
        if index >= entry.data.len() || !entry.kind.matches(value) {
            return None;
        }
        Some(std::mem::replace(&mut entry.data[index], value))
    }) else {
        return Value::nil();
    };
    let Some(old_value) = old_value else {
        return Value::nil();
    };
    unsafe {
        wr_rc_inc(value);
        wr_rc_dec(old_value);
    }
    Value::nil()
}

pub fn dispatch_begin(
    num_workgroups_x: Value,
    num_workgroups_y: Value,
    num_workgroups_z: Value,
    workgroup_size_x: Value,
    workgroup_size_y: Value,
    workgroup_size_z: Value,
    schedule: Value,
) -> Value {
    let num_workgroups = [
        value_to_u32(num_workgroups_x),
        value_to_u32(num_workgroups_y),
        value_to_u32(num_workgroups_z),
    ];
    let workgroup_size = [
        value_to_u32(workgroup_size_x),
        value_to_u32(workgroup_size_y),
        value_to_u32(workgroup_size_z),
    ];
    let total_size = [
        num_workgroups[0].saturating_mul(workgroup_size[0]),
        num_workgroups[1].saturating_mul(workgroup_size[1]),
        num_workgroups[2].saturating_mul(workgroup_size[2]),
    ];
    let total_count = total_size
        .iter()
        .try_fold(1usize, |acc, value| {
            acc.checked_mul(usize::try_from(*value).ok()?)
        })
        .unwrap_or(0);
    DISPATCH_STACK.with(|stack| {
        let mut state = DispatchState {
            num_workgroups,
            workgroup_size,
            total_size,
            total_count,
            schedule: ActiveDispatchSchedule::Deterministic,
            workgroup_id: [0, 0, 0],
            local_id: [0, 0, 0],
        };
        state.schedule = materialize_schedule(schedule, &state);
        stack.borrow_mut().push(state);
    });
    Value::nil()
}

pub fn dispatch_select_invocation(index: Value) -> Value {
    let Some(index) = value_to_len(index) else {
        return Value::nil();
    };
    let updated = with_current_dispatch_mut(|state| {
        if index >= state.total_count {
            state.workgroup_id = [0, 0, 0];
            state.local_id = [0, 0, 0];
            return;
        }
        let scheduled = scheduled_linear_index(state, index);
        let global = decode_global_id(state, scheduled);
        state.workgroup_id = [
            safe_div(global[0], state.workgroup_size[0]),
            safe_div(global[1], state.workgroup_size[1]),
            safe_div(global[2], state.workgroup_size[2]),
        ];
        state.local_id = [
            safe_mod(global[0], state.workgroup_size[0]),
            safe_mod(global[1], state.workgroup_size[1]),
            safe_mod(global[2], state.workgroup_size[2]),
        ];
    });
    if !updated {
        return Value::nil();
    }
    Value::nil()
}

pub fn dispatch_end() -> Value {
    DISPATCH_STACK.with(|stack| {
        let _ = stack.borrow_mut().pop();
    });
    Value::nil()
}

pub fn global_invocation_id() -> Value {
    with_current_dispatch(|state| build_int_list3(global_id(state)))
        .unwrap_or_else(|| build_int_list3([0, 0, 0]))
}

pub fn local_invocation_id() -> Value {
    with_current_dispatch(|state| build_int_list3(state.local_id))
        .unwrap_or_else(|| build_int_list3([0, 0, 0]))
}

pub fn workgroup_id() -> Value {
    with_current_dispatch(|state| build_int_list3(state.workgroup_id))
        .unwrap_or_else(|| build_int_list3([0, 0, 0]))
}

pub fn num_workgroups() -> Value {
    with_current_dispatch(|state| build_int_list3(state.num_workgroups))
        .unwrap_or_else(|| build_int_list3([0, 0, 0]))
}

pub fn workgroup_size() -> Value {
    with_current_dispatch(|state| build_int_list3(state.workgroup_size))
        .unwrap_or_else(|| build_int_list3([0, 0, 0]))
}

fn materialize_schedule(schedule: Value, state: &DispatchState) -> ActiveDispatchSchedule {
    let spec = decode_schedule(schedule).unwrap_or(DispatchScheduleSpec::Deterministic);
    match spec {
        DispatchScheduleSpec::Deterministic => ActiveDispatchSchedule::Deterministic,
        DispatchScheduleSpec::Reverse => ActiveDispatchSchedule::Reverse,
        DispatchScheduleSpec::Shuffle(seed) => ActiveDispatchSchedule::Shuffle(
            build_shuffle_order(state.total_count, seed.into()).into_boxed_slice(),
        ),
        DispatchScheduleSpec::WorkgroupReverse => ActiveDispatchSchedule::WorkgroupReverse,
        DispatchScheduleSpec::WorkgroupShuffle(seed) => ActiveDispatchSchedule::WorkgroupShuffle(
            build_workgroup_order(state, seed, false).into_boxed_slice(),
        ),
        DispatchScheduleSpec::RoundRobinWorkgroups => ActiveDispatchSchedule::RoundRobinWorkgroups(
            build_workgroup_order(state, 0, true).into_boxed_slice(),
        ),
    }
}

fn build_shuffle_order(total_count: usize, seed: u64) -> Vec<u32> {
    let limit = total_count.min(u32::MAX as usize);
    let mut order = (0..limit)
        .map(|value| u32::try_from(value).expect("shuffle order index"))
        .collect::<Vec<_>>();
    let mut state = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
    for idx in (1..order.len()).rev() {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let swap_idx = (state % (idx as u64 + 1)) as usize;
        order.swap(idx, swap_idx);
    }
    order
}

fn build_workgroup_order(state: &DispatchState, seed: u32, round_robin: bool) -> Vec<u32> {
    let group_count = state.num_workgroups.iter().try_fold(1usize, |acc, value| {
        acc.checked_mul(usize::try_from(*value).ok()?)
    });
    let Some(group_count) = group_count else {
        return Vec::new();
    };
    if group_count == 0 {
        return Vec::new();
    }
    let local_volume = state.workgroup_size.iter().try_fold(1usize, |acc, value| {
        acc.checked_mul(usize::try_from(*value).ok()?)
    });
    let Some(local_volume) = local_volume else {
        return Vec::new();
    };
    if local_volume == 0 {
        return Vec::new();
    }

    let mut groups = (0..group_count)
        .map(|value| u32::try_from(value).expect("workgroup order index"))
        .collect::<Vec<_>>();
    if seed != 0 {
        let mut state = seed as u64;
        for idx in (1..groups.len()).rev() {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let swap_idx = (state % (idx as u64 + 1)) as usize;
            groups.swap(idx, swap_idx);
        }
    }
    let mut order = Vec::with_capacity(state.total_count);
    if round_robin {
        for local_linear in 0..local_volume {
            for &group_linear in &groups {
                let group_linear = group_linear as usize;
                let global_linear =
                    workgroup_local_to_global_linear(state, group_linear, local_linear)
                        .and_then(|value| u32::try_from(value).ok());
                if let Some(global_linear) = global_linear {
                    order.push(global_linear);
                }
            }
        }
    } else {
        for &group_linear in &groups {
            let group_linear = group_linear as usize;
            for local_linear in 0..local_volume {
                let global_linear =
                    workgroup_local_to_global_linear(state, group_linear, local_linear)
                        .and_then(|value| u32::try_from(value).ok());
                if let Some(global_linear) = global_linear {
                    order.push(global_linear);
                }
            }
        }
    }
    order
}

fn scheduled_linear_index(state: &DispatchState, logical_index: usize) -> usize {
    match &state.schedule {
        ActiveDispatchSchedule::Deterministic => logical_index,
        ActiveDispatchSchedule::Reverse => state.total_count.saturating_sub(logical_index + 1),
        ActiveDispatchSchedule::Shuffle(order) => order
            .get(logical_index)
            .copied()
            .map(|value| value as usize)
            .unwrap_or(logical_index),
        ActiveDispatchSchedule::WorkgroupReverse => {
            scheduled_workgroup_linear_index(state, logical_index)
        }
        ActiveDispatchSchedule::WorkgroupShuffle(order)
        | ActiveDispatchSchedule::RoundRobinWorkgroups(order) => order
            .get(logical_index)
            .copied()
            .map(|value| value as usize)
            .unwrap_or(logical_index),
    }
}

fn scheduled_workgroup_linear_index(state: &DispatchState, logical_index: usize) -> usize {
    let workgroup_volume = state.workgroup_size.iter().try_fold(1usize, |acc, value| {
        acc.checked_mul(usize::try_from(*value).ok()?)
    });
    let Some(workgroup_volume) = workgroup_volume else {
        return logical_index;
    };
    let workgroup_count = state.num_workgroups.iter().try_fold(1usize, |acc, value| {
        acc.checked_mul(usize::try_from(*value).ok()?)
    });
    let Some(workgroup_count) = workgroup_count else {
        return logical_index;
    };
    if workgroup_volume == 0 || workgroup_count == 0 {
        return logical_index;
    }

    let group_linear = logical_index / workgroup_volume;
    let local_linear = logical_index % workgroup_volume;
    let actual_group = workgroup_count.saturating_sub(group_linear + 1);

    workgroup_local_to_global_linear(state, actual_group, local_linear).unwrap_or(logical_index)
}

fn decode_global_id(state: &DispatchState, linear_index: usize) -> [u32; 3] {
    decode_linear_coords(state.total_size, linear_index)
}

fn decode_linear_coords(extents: [u32; 3], linear_index: usize) -> [u32; 3] {
    let extent_x = usize::try_from(extents[0]).ok().unwrap_or(0);
    let extent_y = usize::try_from(extents[1]).ok().unwrap_or(0);
    if extent_x == 0 || extent_y == 0 {
        return [0, 0, 0];
    }
    let x = linear_index % extent_x;
    let yz_linear = linear_index / extent_x;
    let y = yz_linear % extent_y;
    let z = yz_linear / extent_y;
    [
        u32::try_from(x).ok().unwrap_or(0),
        u32::try_from(y).ok().unwrap_or(0),
        u32::try_from(z).ok().unwrap_or(0),
    ]
}

fn encode_linear_coords(extents: [u32; 3], coords: [u32; 3]) -> Option<usize> {
    if coords[0] >= extents[0] || coords[1] >= extents[1] || coords[2] >= extents[2] {
        return None;
    }
    let extent_x = usize::try_from(extents[0]).ok()?;
    let extent_y = usize::try_from(extents[1]).ok()?;
    let x = usize::try_from(coords[0]).ok()?;
    let y = usize::try_from(coords[1]).ok()?;
    let z = usize::try_from(coords[2]).ok()?;
    z.checked_mul(extent_y)?
        .checked_add(y)?
        .checked_mul(extent_x)?
        .checked_add(x)
}

fn workgroup_local_to_global_linear(
    state: &DispatchState,
    group_linear: usize,
    local_linear: usize,
) -> Option<usize> {
    let group_coords = decode_linear_coords(state.num_workgroups, group_linear);
    let local_coords = decode_linear_coords(state.workgroup_size, local_linear);
    let global_coords = [
        group_coords[0]
            .checked_mul(state.workgroup_size[0])?
            .checked_add(local_coords[0])?,
        group_coords[1]
            .checked_mul(state.workgroup_size[1])?
            .checked_add(local_coords[1])?,
        group_coords[2]
            .checked_mul(state.workgroup_size[2])?
            .checked_add(local_coords[2])?,
    ];
    encode_linear_coords(state.total_size, global_coords)
}

fn safe_div(value: u32, divisor: u32) -> u32 {
    if divisor == 0 { 0 } else { value / divisor }
}

fn safe_mod(value: u32, divisor: u32) -> u32 {
    if divisor == 0 { 0 } else { value % divisor }
}

#[cfg(test)]
mod tests;

pub fn gpu_atomic_i32_new(initial: Value) -> Value {
    let Some(initial) = value_to_i32_strict(initial) else {
        return Value::nil();
    };
    gpu_atomic_i32_registry()
        .insert(initial)
        .map(Value::from_int)
        .unwrap_or_else(Value::nil)
}

pub fn gpu_atomic_i32_drop(handle: Value) -> Value {
    Value::from_bool(gpu_atomic_i32_registry().remove(int_value(handle).unwrap_or(0)))
}

pub fn gpu_atomic_i32_load(handle: Value) -> Value {
    let Some(cell) = gpu_atomic_i32_registry().get(int_value(handle).unwrap_or(0)) else {
        return Value::nil();
    };
    Value::from_int(cell.load(Ordering::SeqCst) as i64)
}

pub fn gpu_atomic_i32_store(handle: Value, value: Value) -> Value {
    let Some(cell) = gpu_atomic_i32_registry().get(int_value(handle).unwrap_or(0)) else {
        return Value::nil();
    };
    let Some(value) = value_to_i32_strict(value) else {
        return Value::nil();
    };
    cell.store(value, Ordering::SeqCst);
    Value::nil()
}

pub fn gpu_atomic_i32_fetch_add(handle: Value, delta: Value) -> Value {
    let Some(cell) = gpu_atomic_i32_registry().get(int_value(handle).unwrap_or(0)) else {
        return Value::nil();
    };
    let Some(delta) = value_to_i32_strict(delta) else {
        return Value::nil();
    };
    Value::from_int(cell.fetch_add(delta, Ordering::SeqCst) as i64)
}

pub fn gpu_atomic_u32_new(initial: Value) -> Value {
    let Some(initial) = value_to_u32_strict(initial) else {
        return Value::nil();
    };
    gpu_atomic_u32_registry()
        .insert(initial)
        .map(Value::from_int)
        .unwrap_or_else(Value::nil)
}

pub fn gpu_atomic_u32_drop(handle: Value) -> Value {
    Value::from_bool(gpu_atomic_u32_registry().remove(int_value(handle).unwrap_or(0)))
}

pub fn gpu_atomic_u32_load(handle: Value) -> Value {
    let Some(cell) = gpu_atomic_u32_registry().get(int_value(handle).unwrap_or(0)) else {
        return Value::nil();
    };
    Value::from_int(cell.load(Ordering::SeqCst) as i64)
}

pub fn gpu_atomic_u32_store(handle: Value, value: Value) -> Value {
    let Some(cell) = gpu_atomic_u32_registry().get(int_value(handle).unwrap_or(0)) else {
        return Value::nil();
    };
    let Some(value) = value_to_u32_strict(value) else {
        return Value::nil();
    };
    cell.store(value, Ordering::SeqCst);
    Value::nil()
}

pub fn gpu_atomic_u32_fetch_add(handle: Value, delta: Value) -> Value {
    let Some(cell) = gpu_atomic_u32_registry().get(int_value(handle).unwrap_or(0)) else {
        return Value::nil();
    };
    let Some(delta) = value_to_u32_strict(delta) else {
        return Value::nil();
    };
    Value::from_int(cell.fetch_add(delta, Ordering::SeqCst) as i64)
}

pub fn gpu_schedule_deterministic() -> Value {
    encode_schedule(DispatchScheduleSpec::Deterministic)
}

pub fn gpu_schedule_reverse() -> Value {
    encode_schedule(DispatchScheduleSpec::Reverse)
}

pub fn gpu_schedule_shuffle(seed: Value) -> Value {
    let Some(seed) = value_to_u32_strict(seed) else {
        return Value::nil();
    };
    encode_schedule(DispatchScheduleSpec::Shuffle(seed))
}

pub fn gpu_schedule_workgroup_reverse() -> Value {
    encode_schedule(DispatchScheduleSpec::WorkgroupReverse)
}

pub fn gpu_schedule_workgroup_shuffle(seed: Value) -> Value {
    let Some(seed) = value_to_u32_strict(seed) else {
        return Value::nil();
    };
    encode_schedule(DispatchScheduleSpec::WorkgroupShuffle(seed))
}

pub fn gpu_schedule_round_robin_workgroups() -> Value {
    encode_schedule(DispatchScheduleSpec::RoundRobinWorkgroups)
}

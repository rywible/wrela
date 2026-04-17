use super::*;

#[test]
fn gpu_atomic_i32_round_trip() {
    let handle = gpu_atomic_i32_new(Value::from_int(-12));
    assert!(handle.is_int());
    assert!(int_value(handle).unwrap() > 0);
    assert_eq!(int_value(gpu_atomic_i32_load(handle)), Some(-12));
    assert_eq!(
        int_value(gpu_atomic_i32_fetch_add(handle, Value::from_int(7))),
        Some(-12)
    );
    assert_eq!(int_value(gpu_atomic_i32_load(handle)), Some(-5));
    assert!(gpu_atomic_i32_store(handle, Value::from_int(41)).is_nil());
    assert_eq!(int_value(gpu_atomic_i32_load(handle)), Some(41));
    assert!(gpu_atomic_i32_drop(handle) == Value::from_bool(true));
    assert!(gpu_atomic_i32_load(handle).is_nil());
    assert!(gpu_atomic_i32_store(handle, Value::from_int(1)).is_nil());
    assert!(gpu_atomic_i32_fetch_add(handle, Value::from_int(1)).is_nil());
}

#[test]
fn gpu_atomic_u32_round_trip() {
    let handle = gpu_atomic_u32_new(Value::from_int(9));
    assert!(handle.is_int());
    assert!(int_value(handle).unwrap() > 0);
    assert_eq!(int_value(gpu_atomic_u32_load(handle)), Some(9));
    assert_eq!(
        int_value(gpu_atomic_u32_fetch_add(handle, Value::from_int(3))),
        Some(9)
    );
    assert_eq!(int_value(gpu_atomic_u32_load(handle)), Some(12));
    assert!(gpu_atomic_u32_store(handle, Value::from_int(77)).is_nil());
    assert_eq!(int_value(gpu_atomic_u32_load(handle)), Some(77));
    assert!(gpu_atomic_u32_drop(handle) == Value::from_bool(true));
    assert!(gpu_atomic_u32_load(handle).is_nil());
    assert!(gpu_atomic_u32_store(handle, Value::from_int(1)).is_nil());
    assert!(gpu_atomic_u32_fetch_add(handle, Value::from_int(1)).is_nil());
}

#[test]
fn gpu_atomic_rejects_invalid_inputs() {
    assert!(gpu_atomic_i32_new(Value::from_float(1.5)).is_nil());
    assert!(gpu_atomic_u32_new(Value::from_float(2.25)).is_nil());
    assert!(gpu_atomic_u32_new(Value::from_int(-1)).is_nil());
}

#[test]
fn dispatch_reverse_schedule_selects_invocations_in_reverse_order() {
    let schedule = gpu_schedule_reverse();
    dispatch_begin(
        Value::from_int(2),
        Value::from_int(1),
        Value::from_int(1),
        Value::from_int(2),
        Value::from_int(1),
        Value::from_int(1),
        schedule,
    );

    dispatch_select_invocation(Value::from_int(0));
    assert_eq!(
        int_value(list::list_get(global_invocation_id(), 0)),
        Some(3)
    );
    assert_eq!(int_value(list::list_get(workgroup_id(), 0)), Some(1));
    assert_eq!(int_value(list::list_get(local_invocation_id(), 0)), Some(1));

    dispatch_select_invocation(Value::from_int(3));
    assert_eq!(
        int_value(list::list_get(global_invocation_id(), 0)),
        Some(0)
    );
    assert_eq!(int_value(list::list_get(workgroup_id(), 0)), Some(0));
    assert_eq!(int_value(list::list_get(local_invocation_id(), 0)), Some(0));

    dispatch_end();
}

#[test]
fn dispatch_shuffle_schedule_is_seed_stable() {
    let collect = || {
        let schedule = gpu_schedule_shuffle(Value::from_int(7));
        dispatch_begin(
            Value::from_int(2),
            Value::from_int(1),
            Value::from_int(1),
            Value::from_int(2),
            Value::from_int(1),
            Value::from_int(1),
            schedule,
        );

        let mut observed = Vec::new();
        for idx in 0..4 {
            dispatch_select_invocation(Value::from_int(idx));
            observed.push(int_value(list::list_get(global_invocation_id(), 0)).unwrap_or(-1));
        }
        dispatch_end();
        observed
    };

    let first = collect();
    let second = collect();
    let mut sorted = first.clone();
    sorted.sort_unstable();

    assert_eq!(first, second);
    assert_eq!(sorted, vec![0, 1, 2, 3]);
}

#[test]
fn dispatch_workgroup_reverse_preserves_local_order() {
    let schedule = gpu_schedule_workgroup_reverse();
    dispatch_begin(
        Value::from_int(2),
        Value::from_int(1),
        Value::from_int(1),
        Value::from_int(2),
        Value::from_int(1),
        Value::from_int(1),
        schedule,
    );

    let mut observed = Vec::new();
    for idx in 0..4 {
        dispatch_select_invocation(Value::from_int(idx));
        observed.push((
            int_value(list::list_get(global_invocation_id(), 0)).unwrap_or(-1),
            int_value(list::list_get(workgroup_id(), 0)).unwrap_or(-1),
            int_value(list::list_get(local_invocation_id(), 0)).unwrap_or(-1),
        ));
    }
    dispatch_end();

    assert_eq!(observed, vec![(2, 1, 0), (3, 1, 1), (0, 0, 0), (1, 0, 1)]);
}

#[test]
fn dispatch_round_robin_workgroups_interleaves_groups() {
    let schedule = gpu_schedule_round_robin_workgroups();
    dispatch_begin(
        Value::from_int(2),
        Value::from_int(1),
        Value::from_int(1),
        Value::from_int(2),
        Value::from_int(1),
        Value::from_int(1),
        schedule,
    );

    let mut observed = Vec::new();
    for idx in 0..4 {
        dispatch_select_invocation(Value::from_int(idx));
        observed.push((
            int_value(list::list_get(global_invocation_id(), 0)).unwrap_or(-1),
            int_value(list::list_get(workgroup_id(), 0)).unwrap_or(-1),
            int_value(list::list_get(local_invocation_id(), 0)).unwrap_or(-1),
        ));
    }
    dispatch_end();

    assert_eq!(observed, vec![(0, 0, 0), (2, 1, 0), (1, 0, 1), (3, 1, 1)]);
}

#[test]
fn dispatch_workgroup_shuffle_is_seed_stable_and_group_local_ordered() {
    let collect = || {
        let schedule = gpu_schedule_workgroup_shuffle(Value::from_int(7));
        dispatch_begin(
            Value::from_int(4),
            Value::from_int(1),
            Value::from_int(1),
            Value::from_int(2),
            Value::from_int(1),
            Value::from_int(1),
            schedule,
        );

        let mut observed = Vec::new();
        for idx in 0..8 {
            dispatch_select_invocation(Value::from_int(idx));
            observed.push((
                int_value(list::list_get(global_invocation_id(), 0)).unwrap_or(-1),
                int_value(list::list_get(workgroup_id(), 0)).unwrap_or(-1),
                int_value(list::list_get(local_invocation_id(), 0)).unwrap_or(-1),
            ));
        }
        dispatch_end();
        observed
    };

    let first = collect();
    let second = collect();
    let mut sorted = first.iter().map(|entry| entry.0).collect::<Vec<_>>();
    sorted.sort_unstable();

    assert_eq!(first, second);
    assert_eq!(sorted, vec![0, 1, 2, 3, 4, 5, 6, 7]);
    for chunk in first.chunks(2) {
        assert_eq!(chunk[0].1, chunk[1].1);
        assert_eq!(chunk[0].2, 0);
        assert_eq!(chunk[1].2, 1);
    }
}

#[test]
fn dispatch_workgroup_reverse_handles_two_dimensional_grids() {
    let schedule = gpu_schedule_workgroup_reverse();
    dispatch_begin(
        Value::from_int(2),
        Value::from_int(2),
        Value::from_int(1),
        Value::from_int(2),
        Value::from_int(2),
        Value::from_int(1),
        schedule,
    );

    let mut observed = Vec::new();
    for idx in 0..4 {
        dispatch_select_invocation(Value::from_int(idx));
        observed.push((
            int_value(list::list_get(global_invocation_id(), 0)).unwrap_or(-1),
            int_value(list::list_get(global_invocation_id(), 1)).unwrap_or(-1),
            int_value(list::list_get(workgroup_id(), 0)).unwrap_or(-1),
            int_value(list::list_get(workgroup_id(), 1)).unwrap_or(-1),
            int_value(list::list_get(local_invocation_id(), 0)).unwrap_or(-1),
            int_value(list::list_get(local_invocation_id(), 1)).unwrap_or(-1),
        ));
    }
    dispatch_end();

    assert_eq!(
        observed,
        vec![
            (2, 2, 1, 1, 0, 0),
            (3, 2, 1, 1, 1, 0),
            (2, 3, 1, 1, 0, 1),
            (3, 3, 1, 1, 1, 1),
        ]
    );
}

#[test]
fn dispatch_round_robin_workgroups_handles_two_dimensional_grids() {
    let schedule = gpu_schedule_round_robin_workgroups();
    dispatch_begin(
        Value::from_int(2),
        Value::from_int(2),
        Value::from_int(1),
        Value::from_int(2),
        Value::from_int(2),
        Value::from_int(1),
        schedule,
    );

    let mut observed = Vec::new();
    for idx in 0..4 {
        dispatch_select_invocation(Value::from_int(idx));
        observed.push((
            int_value(list::list_get(global_invocation_id(), 0)).unwrap_or(-1),
            int_value(list::list_get(global_invocation_id(), 1)).unwrap_or(-1),
            int_value(list::list_get(workgroup_id(), 0)).unwrap_or(-1),
            int_value(list::list_get(workgroup_id(), 1)).unwrap_or(-1),
            int_value(list::list_get(local_invocation_id(), 0)).unwrap_or(-1),
            int_value(list::list_get(local_invocation_id(), 1)).unwrap_or(-1),
        ));
    }
    dispatch_end();

    assert_eq!(
        observed,
        vec![
            (0, 0, 0, 0, 0, 0),
            (2, 0, 1, 0, 0, 0),
            (0, 2, 0, 1, 0, 0),
            (2, 2, 1, 1, 0, 0),
        ]
    );
}

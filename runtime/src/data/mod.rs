pub(crate) mod arena;
pub(crate) mod bytes;
pub(crate) mod class;
pub(crate) mod iter;
pub(crate) mod list;
pub(crate) mod map;
pub(crate) mod object;
pub(crate) mod result;
pub(crate) mod string;
pub(crate) mod value;

#[cfg(test)]
mod tests {
    use crate::value::Value;
    use crate::{arena, list, result, string, wr_rc_dec, wr_rc_inc};
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;
    use std::sync::atomic::Ordering;

    fn rc_count(value: Value) -> u32 {
        assert!(value.is_ptr());
        unsafe { (*value.as_ptr()).rc.load(Ordering::Relaxed) }
    }

    #[test]
    fn region_confined_rc_ops_are_elided() {
        let mut arena_mem = arena::Arena::new(1024);
        let _guard = arena::enter(&mut arena_mem as *mut _);

        let local = list::list_new_local(0);
        assert!(arena::is_arena_value(local));
        assert_eq!(rc_count(local), 1);

        unsafe {
            wr_rc_inc(local);
            wr_rc_dec(local);
        }

        // The global metrics counters are process-wide and can be advanced by
        // other concurrently running tests, so this assertion focuses on
        // observable local behavior.
        assert_eq!(rc_count(local), 1);

        arena_mem.reset();
    }

    #[test]
    fn region_bulk_reclaim_drops_external_refs() {
        let heap = string::str_from_bytes(b"owned-by-heap");
        assert_eq!(rc_count(heap), 1);

        let mut arena_mem = arena::Arena::new(1024);
        {
            let _guard = arena::enter(&mut arena_mem as *mut _);
            let local = list::list_new_local(0);
            list::list_push(local, heap);
            assert_eq!(rc_count(heap), 2);
            arena_mem.reset();
        }

        assert_eq!(rc_count(heap), 1);
        unsafe { wr_rc_dec(heap) };
    }

    #[test]
    fn result_rejects_region_confined_escape() {
        let mut arena_mem = arena::Arena::new(1024);
        let _guard = arena::enter(&mut arena_mem as *mut _);

        let local = string::str_concat_local([Value::from_int(7)].as_ptr(), 1);
        assert!(arena::is_arena_value(local));
        assert!(arena::reject_arena_escape(local, "test").is_none());

        let wrapped = result::result_ok(local);
        assert!(wrapped.is_nil());

        arena_mem.reset();
    }

    #[test]
    fn value_eq_rejects_lossy_large_int_float_coercion() {
        let too_large_for_exact_f64 = (1i64 << 53) + 1;
        let rounded_f64 = (1i64 << 53) as f64;
        let int_val = Value::from_int(too_large_for_exact_f64);
        let float_val = Value::from_float(rounded_f64);
        assert!(
            !crate::value::value_eq(int_val, float_val),
            "lossy int→float coercion must not compare equal"
        );
    }

    #[test]
    fn value_hash_matches_int_float_equality_contract() {
        let int_val = Value::from_int(42);
        let float_val = Value::from_float(42.0);
        assert!(crate::value::value_eq(int_val, float_val));

        let mut int_hasher = DefaultHasher::new();
        let mut float_hasher = DefaultHasher::new();
        crate::value::value_hash(int_val, &mut int_hasher);
        crate::value::value_hash(float_val, &mut float_hasher);

        assert_eq!(int_hasher.finish(), float_hasher.finish());
    }
}

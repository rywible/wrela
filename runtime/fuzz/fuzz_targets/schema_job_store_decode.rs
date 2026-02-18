#![no_main]

use libfuzzer_sys::fuzz_target;
use wrela_runtime::db::schema_evolution::SchemaJobStore;
use wrela_runtime_fuzz::cap_input;

const MAX_INPUT_BYTES: usize = 32 * 1024;

fuzz_target!(|data: &[u8]| {
    let bounded = cap_input(data, MAX_INPUT_BYTES);
    if let Ok(store) = SchemaJobStore::from_canonical_bytes(bounded) {
        let _ = store.to_canonical_bytes();
    }
});

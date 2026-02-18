#![no_main]

use libfuzzer_sys::fuzz_target;
use wrela_runtime::db::wal::format::decode_at;
use wrela_runtime_fuzz::cap_input;

const MAX_INPUT_BYTES: usize = 64 * 1024;
const MAX_RECORDS_PER_INPUT: usize = 256;

fuzz_target!(|data: &[u8]| {
    let bounded = cap_input(data, MAX_INPUT_BYTES);
    let mut offset = 0usize;
    let mut decoded = 0usize;
    while decoded < MAX_RECORDS_PER_INPUT {
        match decode_at(bounded, offset) {
            Ok(Some((_, next))) if next > offset => {
                offset = next;
                decoded += 1;
            }
            Ok(Some((_record, _next))) => break,
            Ok(None) => break,
            Err(_) => break,
        }
    }
});

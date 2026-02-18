#![no_main]

use libfuzzer_sys::fuzz_target;
use wrela_runtime::db::sql::parse_statement;
use wrela_runtime_fuzz::cap_input;

const MAX_SQL_BYTES: usize = 4096;

fuzz_target!(|data: &[u8]| {
    let bounded = cap_input(data, MAX_SQL_BYTES);
    if let Ok(sql) = std::str::from_utf8(bounded) {
        let _ = parse_statement(sql);
    }
});

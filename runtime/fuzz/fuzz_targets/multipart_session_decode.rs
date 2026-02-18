#![no_main]

use libfuzzer_sys::fuzz_target;
use wrela_runtime::db::backup::MultipartUploadSession;
use wrela_runtime_fuzz::cap_input;

const MAX_INPUT_BYTES: usize = 16 * 1024;

fuzz_target!(|data: &[u8]| {
    let bounded = cap_input(data, MAX_INPUT_BYTES);
    if let Ok(session) = MultipartUploadSession::from_persisted_bytes(bounded) {
        let _ = session.progress();
        let _ = session.to_persisted_bytes();
    }
});

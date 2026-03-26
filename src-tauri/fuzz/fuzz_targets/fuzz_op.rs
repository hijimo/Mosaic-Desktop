#![no_main]
//! F-02: Fuzz Op serialization/deserialization roundtrip.

use libfuzzer_sys::fuzz_target;
use tauri_app_lib::protocol::submission::Op;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        if let Ok(op) = serde_json::from_str::<Op>(s) {
            if let Ok(json) = serde_json::to_string(&op) {
                let _ = serde_json::from_str::<Op>(&json);
            }
        }
    }
});

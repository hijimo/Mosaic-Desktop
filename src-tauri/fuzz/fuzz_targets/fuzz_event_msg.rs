#![no_main]
//! F-01: Fuzz EventMsg serialization/deserialization roundtrip.
//! Ensures no panic on arbitrary JSON input.

use libfuzzer_sys::fuzz_target;
use tauri_app_lib::protocol::event::EventMsg;

fuzz_target!(|data: &[u8]| {
    // Try to parse as JSON, then deserialize as EventMsg
    if let Ok(s) = std::str::from_utf8(data) {
        if let Ok(msg) = serde_json::from_str::<EventMsg>(s) {
            // Roundtrip: serialize back and deserialize again
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = serde_json::from_str::<EventMsg>(&json);
            }
        }
    }
});

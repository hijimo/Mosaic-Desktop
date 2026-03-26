#![no_main]
//! F-04: Fuzz TurnItem serialization/deserialization roundtrip.

use libfuzzer_sys::fuzz_target;
use tauri_app_lib::protocol::items::TurnItem;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        if let Ok(item) = serde_json::from_str::<TurnItem>(s) {
            if let Ok(json) = serde_json::to_string(&item) {
                let _ = serde_json::from_str::<TurnItem>(&json);
            }
        }
    }
});

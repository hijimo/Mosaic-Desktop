//! Integration test: verify that the JSON payloads the frontend sends
//! can be correctly deserialized by the Rust backend.

#[cfg(test)]
mod frontend_op_compat {
    use crate::protocol::submission::Op;

    #[test]
    fn frontend_user_turn_json_deserializes() {
        // This is the exact JSON shape our frontend sends via submitOp
        let json = r#"{
            "type": "user_turn",
            "items": [
                { "type": "text", "text": "hello world", "text_elements": [] }
            ],
            "cwd": ".",
            "model": "",
            "approval_policy": "on-request",
            "sandbox_policy": { "type": "danger-full-access" }
        }"#;

        let op: Op = serde_json::from_str(json).expect("frontend user_turn JSON should deserialize");
        match op {
            Op::UserTurn { items, cwd, model, .. } => {
                assert_eq!(items.len(), 1);
                assert_eq!(cwd.to_string_lossy(), ".");
                assert_eq!(model, "");
            }
            other => panic!("expected UserTurn, got {:?}", other),
        }
    }

    #[test]
    fn frontend_interrupt_json_deserializes() {
        let json = r#"{ "type": "interrupt" }"#;
        let op: Op = serde_json::from_str(json).expect("interrupt should deserialize");
        assert!(matches!(op, Op::Interrupt));
    }

    #[test]
    fn frontend_shutdown_json_deserializes() {
        let json = r#"{ "type": "shutdown" }"#;
        let op: Op = serde_json::from_str(json).expect("shutdown should deserialize");
        assert!(matches!(op, Op::Shutdown));
    }

    #[test]
    fn frontend_set_thread_name_json_deserializes() {
        let json = r#"{ "type": "set_thread_name", "name": "My Chat" }"#;
        let op: Op = serde_json::from_str(json).expect("set_thread_name should deserialize");
        match op {
            Op::SetThreadName { name } => assert_eq!(name, "My Chat"),
            other => panic!("expected SetThreadName, got {:?}", other),
        }
    }

    #[test]
    fn frontend_compact_json_deserializes() {
        let json = r#"{ "type": "compact" }"#;
        let op: Op = serde_json::from_str(json).expect("compact should deserialize");
        assert!(matches!(op, Op::Compact));
    }
}

use crate::protocol::types::ResponseInputItem;

/// Ensure every `FunctionCall` has a corresponding `FunctionOutput`.
/// Missing outputs get a synthetic "aborted" entry inserted right after the call.
pub(crate) fn ensure_call_outputs_present(items: &mut Vec<ResponseInputItem>) {
    let mut inserts: Vec<(usize, ResponseInputItem)> = Vec::new();

    for (idx, item) in items.iter().enumerate() {
        if let ResponseInputItem::FunctionCall { call_id, .. } = item {
            let has_output = items.iter().any(|i| {
                matches!(
                    i,
                    ResponseInputItem::FunctionCallOutput { call_id: cid, .. } if cid == call_id
                )
            });
            if !has_output {
                inserts.push((
                    idx,
                    ResponseInputItem::FunctionCallOutput {
                        call_id: call_id.clone(),
                        output: crate::protocol::types::FunctionCallOutputPayload::from_text(
                            "aborted".into(),
                        ),
                    },
                ));
            }
        }
    }

    // Insert in reverse order to avoid index shifting.
    for (idx, output) in inserts.into_iter().rev() {
        items.insert(idx + 1, output);
    }
}

/// Remove `FunctionOutput` entries that have no matching `FunctionCall`.
pub(crate) fn remove_orphan_outputs(items: &mut Vec<ResponseInputItem>) {
    let call_ids: std::collections::HashSet<String> = items
        .iter()
        .filter_map(|i| match i {
            ResponseInputItem::FunctionCall { call_id, .. } => Some(call_id.clone()),
            _ => None,
        })
        .collect();

    items.retain(|item| match item {
        ResponseInputItem::FunctionCallOutput { call_id, .. } => call_ids.contains(call_id),
        _ => true,
    });
}

/// When an item is removed, also remove its corresponding call/output pair.
pub(crate) fn remove_corresponding(
    items: &mut Vec<ResponseInputItem>,
    removed: &ResponseInputItem,
) {
    match removed {
        ResponseInputItem::FunctionCall { call_id, .. } => {
            items.retain(|i| {
                !matches!(
                    i,
                    ResponseInputItem::FunctionCallOutput { call_id: cid, .. } if cid == call_id
                )
            });
        }
        ResponseInputItem::FunctionCallOutput { call_id, .. } => {
            items.retain(|i| {
                !matches!(
                    i,
                    ResponseInputItem::FunctionCall { call_id: cid, .. } if cid == call_id
                )
            });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::types::{ContentOrItems, FunctionCallOutputPayload};

    fn func_call(id: &str) -> ResponseInputItem {
        ResponseInputItem::FunctionCall {
            call_id: id.into(),
            name: "tool".into(),
            arguments: "{}".into(),
        }
    }

    fn func_output(id: &str, text: &str) -> ResponseInputItem {
        ResponseInputItem::FunctionCallOutput {
            call_id: id.into(),
            output: FunctionCallOutputPayload::from_text(text.into()),
        }
    }

    fn user_msg(text: &str) -> ResponseInputItem {
        ResponseInputItem::text_message("user", text.to_string())
    }

    #[test]
    fn adds_missing_output() {
        let mut items = vec![user_msg("hi"), func_call("c1")];
        ensure_call_outputs_present(&mut items);
        assert_eq!(items.len(), 3);
        assert!(
            matches!(&items[2], ResponseInputItem::FunctionCallOutput { call_id, .. } if call_id == "c1")
        );
    }

    #[test]
    fn does_not_duplicate_existing_output() {
        let mut items = vec![func_call("c1"), func_output("c1", "ok")];
        ensure_call_outputs_present(&mut items);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn removes_orphan_output() {
        let mut items = vec![user_msg("hi"), func_output("orphan", "data")];
        remove_orphan_outputs(&mut items);
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], ResponseInputItem::Message { .. }));
    }

    #[test]
    fn keeps_matched_output() {
        let mut items = vec![func_call("c1"), func_output("c1", "ok")];
        remove_orphan_outputs(&mut items);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn remove_corresponding_call_removes_output() {
        let call = func_call("c1");
        let mut items = vec![func_output("c1", "ok"), user_msg("hi")];
        remove_corresponding(&mut items, &call);
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], ResponseInputItem::Message { .. }));
    }

    #[test]
    fn remove_corresponding_output_removes_call() {
        let output = func_output("c1", "ok");
        let mut items = vec![func_call("c1"), user_msg("hi")];
        remove_corresponding(&mut items, &output);
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], ResponseInputItem::Message { .. }));
    }

    #[test]
    fn remove_corresponding_message_is_noop() {
        let msg = user_msg("hi");
        let mut items = vec![func_call("c1"), func_output("c1", "ok")];
        remove_corresponding(&mut items, &msg);
        assert_eq!(items.len(), 2);
    }
}

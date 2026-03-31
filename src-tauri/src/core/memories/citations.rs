//! Extract thread IDs from memory citation blocks.
//!
//! Memory citations are XML-like blocks embedded in model output that
//! reference the threads (rollouts) a memory was derived from.

use crate::protocol::ThreadId;

/// Parse citation blocks and extract valid `ThreadId` values.
///
/// Supports both `<thread_ids>` (current) and `<rollout_ids>` (legacy) tags.
pub fn get_thread_id_from_citations(citations: Vec<String>) -> Vec<ThreadId> {
    let mut result = Vec::new();
    for citation in citations {
        let ids_block = extract_ids_block(&citation);
        if let Some(block) = ids_block {
            for id in block.lines().map(str::trim).filter(|line| !line.is_empty()) {
                if let Ok(thread_id) = ThreadId::try_from(id) {
                    result.push(thread_id);
                }
            }
        }
    }
    result
}

fn extract_ids_block(citation: &str) -> Option<&str> {
    for (open, close) in [
        ("<thread_ids>", "</thread_ids>"),
        ("<rollout_ids>", "</rollout_ids>"),
    ] {
        if let Some(rest) = citation.split_once(open).map(|(_, r)| r) {
            if let Some(ids) = rest.split_once(close).map(|(ids, _)| ids) {
                return Some(ids);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::get_thread_id_from_citations;
    use crate::protocol::ThreadId;

    #[test]
    fn extracts_thread_ids() {
        let first = ThreadId::new();
        let second = ThreadId::new();

        let citations = vec![format!(
            "<memory_citation>\n<citation_entries>\nMEMORY.md:1-2|note=[x]\n</citation_entries>\n<thread_ids>\n{first}\nnot-a-uuid\n{second}\n</thread_ids>\n</memory_citation>"
        )];

        assert_eq!(get_thread_id_from_citations(citations), vec![first, second]);
    }

    #[test]
    fn supports_legacy_rollout_ids() {
        let thread_id = ThreadId::new();

        let citations = vec![format!(
            "<memory_citation>\n<rollout_ids>\n{thread_id}\n</rollout_ids>\n</memory_citation>"
        )];

        assert_eq!(get_thread_id_from_citations(citations), vec![thread_id]);
    }

    #[test]
    fn empty_citations_returns_empty() {
        assert!(get_thread_id_from_citations(vec![]).is_empty());
    }

    #[test]
    fn no_matching_tags_returns_empty() {
        let citations = vec!["no tags here".to_string()];
        assert!(get_thread_id_from_citations(citations).is_empty());
    }

    #[test]
    fn multiple_citations_merged() {
        let id1 = ThreadId::new();
        let id2 = ThreadId::new();

        let citations = vec![
            format!("<thread_ids>\n{id1}\n</thread_ids>"),
            format!("<rollout_ids>\n{id2}\n</rollout_ids>"),
        ];

        let result = get_thread_id_from_citations(citations);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], id1);
        assert_eq!(result[1], id2);
    }
}

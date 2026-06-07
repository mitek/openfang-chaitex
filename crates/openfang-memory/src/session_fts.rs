//! Session FTS5 helpers — stable text flattening for indexable search.
//!
//! Every `Message` shape must produce non-empty indexable text so that a
//! session whose only content is `tool_use` or `image` blocks still appears
//! in future `session_search` results.
//!
//! Used by:
//! - The v8→v9 schema migration backfill (plan 01-02).
//! - `SessionStore::save_session` dual-write path (plan 01-03).
//! - The `session_search` tool implementation (plan 01-04).
//!
//! Determinism invariant: `flatten_message_content(msg)` is byte-stable across
//! re-runs on the same `Message`. The implementation must not rely on
//! HashMap iteration order or random IDs.

use openfang_types::message::{ContentBlock, Message, MessageContent, Role};

/// Maximum bytes of serialized JSON included for a `tool_use` block.
///
/// FTS5 tokenizes text; large JSON inputs (e.g. file contents pasted into
/// arguments) blow up the index for little search benefit. We keep just enough
/// to make the call discoverable by the tool name and the first few arguments.
const TOOL_USE_JSON_MAX_BYTES: usize = 2048;

/// Flatten a `Message` into a single deterministic, FTS5-indexable string.
///
/// Invariants:
/// - A `Message` with at least one block never flattens to an empty string —
///   even pure-image or pure-redacted-thinking messages produce a tag marker.
/// - The output is stable: re-running on the same `Message` yields the same
///   bytes (no map iteration order, no random IDs).
/// - For `MessageContent::Text(s)` the output is `s` verbatim — no markers,
///   so search queries match plain conversational text directly.
pub fn flatten_message_content(msg: &Message) -> String {
    match &msg.content {
        MessageContent::Text(s) => s.clone(),
        MessageContent::Blocks(blocks) => {
            if blocks.is_empty() {
                // Empty Blocks vec is a legitimate edge case (e.g. an
                // assistant turn that ended on stop with no content). We
                // intentionally return "" — there is nothing to index.
                return String::new();
            }
            let mut parts: Vec<String> = Vec::with_capacity(blocks.len());
            for block in blocks {
                parts.push(flatten_block(block));
            }
            parts.join("\n")
        }
    }
}

fn flatten_block(block: &ContentBlock) -> String {
    match block {
        ContentBlock::Text { text, .. } => text.clone(),
        ContentBlock::ToolUse { name, input, .. } => {
            // serde_json::to_string is deterministic for `serde_json::Value`
            // because object members are stored in a BTreeMap (sorted) when
            // the `preserve_order` feature is OFF (it is OFF in this repo).
            // That gives us byte-stable output.
            let raw = serde_json::to_string(input).unwrap_or_else(|_| String::new());
            let truncated = safe_truncate(&raw, TOOL_USE_JSON_MAX_BYTES);
            format!("[tool_use:{}] {}", name, truncated)
        }
        ContentBlock::ToolResult { content, .. } => {
            format!("[tool_result] {}", content)
        }
        ContentBlock::Image { .. } => "[image]".to_string(),
        ContentBlock::Thinking { thinking, .. } => {
            format!("[thinking] {}", thinking)
        }
        ContentBlock::RedactedThinking { .. } => "[redacted_thinking]".to_string(),
        ContentBlock::Unknown => "[unknown_block]".to_string(),
    }
}

/// Truncate `s` at-or-before `max_bytes`, respecting UTF-8 char boundaries.
///
/// If `s.len() <= max_bytes`, returns `s` unchanged. Otherwise finds the
/// largest byte index `<= max_bytes` that lands on a char boundary and
/// returns the slice up to that index. This avoids panicking on multibyte
/// codepoints (em-dashes, emoji, CJK) at the cut point.
fn safe_truncate(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut cut = max_bytes;
    while cut > 0 && !s.is_char_boundary(cut) {
        cut -= 1;
    }
    s[..cut].to_string()
}

/// Map a `Role` to its lowercase string form for the `session_messages.role`
/// column. Used by both the backfill and the dual-write path so the indexed
/// `role` filter is consistent.
///
/// NOTE: the `Role` enum in `openfang-types` currently has only three
/// variants (`System`, `User`, `Assistant`). Tool results are carried as a
/// `ContentBlock::ToolResult` inside a user-role message, not as a separate
/// role. We keep the function shape `&Role -> &'static str` so adding a
/// `Tool` variant in the future is a single-line change here.
pub fn role_string(role: &Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openfang_types::message::{ContentBlock, Message};

    fn assistant_with(blocks: Vec<ContentBlock>) -> Message {
        Message::assistant_with_blocks(blocks)
    }

    #[test]
    fn flatten_plain_text_returns_verbatim() {
        let msg = Message::user("hello world");
        assert_eq!(flatten_message_content(&msg), "hello world");
    }

    #[test]
    fn flatten_text_block() {
        let msg = assistant_with(vec![ContentBlock::Text {
            text: "alpha".to_string(),
            provider_metadata: None,
        }]);
        assert_eq!(flatten_message_content(&msg), "alpha");
    }

    #[test]
    fn flatten_tool_use_block() {
        let msg = assistant_with(vec![ContentBlock::ToolUse {
            id: "tu_1".to_string(),
            name: "file_read".to_string(),
            input: serde_json::json!({"path": "/tmp/x.txt"}),
            provider_metadata: None,
        }]);
        let out = flatten_message_content(&msg);
        assert!(
            out.starts_with("[tool_use:file_read]"),
            "got: {:?}",
            out
        );
        assert!(out.contains("\"path\":\"/tmp/x.txt\""), "got: {:?}", out);
    }

    #[test]
    fn flatten_tool_result_block() {
        let msg = assistant_with(vec![ContentBlock::ToolResult {
            tool_use_id: "tu_1".to_string(),
            tool_name: "file_read".to_string(),
            content: "file contents here".to_string(),
            is_error: false,
        }]);
        assert_eq!(
            flatten_message_content(&msg),
            "[tool_result] file contents here"
        );
    }

    #[test]
    fn flatten_image_block_uses_marker() {
        let msg = assistant_with(vec![ContentBlock::Image {
            media_type: "image/png".to_string(),
            data: "base64data".to_string(),
        }]);
        let out = flatten_message_content(&msg);
        assert_eq!(out, "[image]");
        assert!(
            !out.is_empty(),
            "image-only message must not flatten to empty (FTS-02 invariant)"
        );
    }

    #[test]
    fn flatten_thinking_block() {
        let msg = assistant_with(vec![ContentBlock::Thinking {
            thinking: "step-by-step analysis".to_string(),
            signature: Some("sig".to_string()),
            provider_metadata: None,
        }]);
        assert_eq!(
            flatten_message_content(&msg),
            "[thinking] step-by-step analysis"
        );
    }

    #[test]
    fn flatten_redacted_thinking_block_uses_marker() {
        let msg = assistant_with(vec![ContentBlock::RedactedThinking {
            data: "opaque-encrypted-data".to_string(),
        }]);
        let out = flatten_message_content(&msg);
        assert_eq!(out, "[redacted_thinking]");
        assert!(
            !out.is_empty(),
            "redacted-only message must not flatten to empty (FTS-02 invariant)"
        );
    }

    #[test]
    fn flatten_unknown_block_uses_marker() {
        // ContentBlock::Unknown is the catch-all #[serde(other)] variant.
        // Build one explicitly to exercise the arm.
        let msg = assistant_with(vec![ContentBlock::Unknown]);
        let out = flatten_message_content(&msg);
        assert_eq!(out, "[unknown_block]");
    }

    #[test]
    fn flatten_multiple_blocks_joined_by_newline() {
        let msg = assistant_with(vec![
            ContentBlock::Text {
                text: "intro".to_string(),
                provider_metadata: None,
            },
            ContentBlock::Image {
                media_type: "image/png".to_string(),
                data: "x".to_string(),
            },
            ContentBlock::Text {
                text: "outro".to_string(),
                provider_metadata: None,
            },
        ]);
        assert_eq!(flatten_message_content(&msg), "intro\n[image]\noutro");
    }

    #[test]
    fn flatten_empty_blocks_vec_returns_empty() {
        // An assistant turn with literally zero blocks has nothing to index.
        // The FTS-02 invariant ("never empty for a non-empty message") does
        // NOT apply here because the Blocks vec IS empty.
        let msg = assistant_with(vec![]);
        assert_eq!(flatten_message_content(&msg), "");
    }

    #[test]
    fn flatten_tool_use_truncates_large_json() {
        // Build a JSON input whose serialized form exceeds the 2KB cap.
        let big = "X".repeat(5_000);
        let msg = assistant_with(vec![ContentBlock::ToolUse {
            id: "tu_big".to_string(),
            name: "exec".to_string(),
            input: serde_json::json!({"blob": big}),
            provider_metadata: None,
        }]);
        let out = flatten_message_content(&msg);
        // Cap = TOOL_USE_JSON_MAX_BYTES + "[tool_use:exec] " prefix.
        let max_total = TOOL_USE_JSON_MAX_BYTES + "[tool_use:exec] ".len();
        assert!(
            out.len() <= max_total,
            "expected <= {}, got {}",
            max_total,
            out.len()
        );
        assert!(out.starts_with("[tool_use:exec]"));
    }

    #[test]
    fn flatten_is_deterministic_across_runs() {
        let msg = assistant_with(vec![
            ContentBlock::ToolUse {
                id: "tu_1".to_string(),
                name: "file_read".to_string(),
                input: serde_json::json!({
                    "zebra": 1,
                    "alpha": 2,
                    "middle": [1, 2, 3]
                }),
                provider_metadata: None,
            },
            ContentBlock::Text {
                text: "tail".to_string(),
                provider_metadata: None,
            },
        ]);
        let a = flatten_message_content(&msg);
        let b = flatten_message_content(&msg);
        assert_eq!(a, b, "flattening must be byte-stable");
    }

    #[test]
    fn flatten_tool_use_respects_utf8_boundary_when_truncating() {
        // A multibyte codepoint near the cut must not panic. Build a JSON
        // whose serialization is longer than 2048 bytes and contains an
        // em-dash near the would-be cut point.
        let mut body = String::new();
        // Padding to push the em-dash toward the cut point.
        body.push_str(&"a".repeat(2000));
        body.push_str("—tail—"); // multibyte
        let msg = assistant_with(vec![ContentBlock::ToolUse {
            id: "tu_utf".to_string(),
            name: "echo".to_string(),
            input: serde_json::json!({"body": body}),
            provider_metadata: None,
        }]);
        let out = flatten_message_content(&msg);
        // The only thing that matters: we didn't panic, and the result is
        // valid UTF-8 (every Rust String is — implicit). Sanity check the
        // tool_use marker is intact.
        assert!(out.starts_with("[tool_use:echo]"));
    }

    #[test]
    fn role_string_lowercase() {
        assert_eq!(role_string(&Role::System), "system");
        assert_eq!(role_string(&Role::User), "user");
        assert_eq!(role_string(&Role::Assistant), "assistant");
    }

    #[test]
    fn fts5_is_compiled_in() {
        // FTS-01: the bundled SQLite amalgamation must report ENABLE_FTS5
        // among its compile options, and a FTS5 virtual table must be
        // createable. rusqlite 0.31 has no top-level `fts5` Cargo feature —
        // FTS5 ships in the bundled libsqlite3-sys amalgamation via the
        // `-DSQLITE_ENABLE_FTS5` compile flag. This probe asserts the
        // capability is wired so plans 01-02/03/04 can rely on it.
        let conn = rusqlite::Connection::open_in_memory().expect("open in-memory db");
        let opt: String = conn
            .query_row(
                "SELECT compile_options FROM pragma_compile_options \
                 WHERE compile_options = 'ENABLE_FTS5'",
                [],
                |r| r.get(0),
            )
            .expect("ENABLE_FTS5 must appear in pragma_compile_options");
        assert_eq!(opt, "ENABLE_FTS5");
        // Concrete capability check — fail loud if FTS5 SQL would fail.
        conn.execute(
            "CREATE VIRTUAL TABLE probe_fts USING fts5(content)",
            [],
        )
        .expect("CREATE VIRTUAL TABLE ... USING fts5 must succeed");
    }
}

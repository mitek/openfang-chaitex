//! KernelHandle-backed implementation of `ReasoningLlm`.
//!
//! `openfang-reasoning` doesn't depend on `openfang-runtime`/`openfang-kernel`
//! (that would invert the crate DAG), so we can't talk to the agent's
//! `LlmDriver` directly. Instead, we define a tiny `KernelLlm` trait here
//! that captures the single capability we need — "synthesize text + give
//! me real token usage" — and the kernel implements it in
//! `openfang-kernel`.
//!
//! `KernelLlmAdapter` wraps a `Weak<K>` (mirrors the `self_handle:
//! Weak<OpenFangKernel>` pattern used elsewhere in the kernel, per
//! CONVENTIONS.md) so the engine never keeps the kernel alive past its
//! shutdown.
//!
//! At synthesis time the adapter:
//! 1. Upgrades the `Weak<K>` to `Arc<K>`; failure → `ReasoningError::Llm`
//!    ("kernel dropped").
//! 2. Builds a level-tuned prompt from the query + retrieved facts,
//!    safely truncated to `max_input_tokens * 4` characters (~ token
//!    heuristic from plan 01-13).
//! 3. Calls `K::complete_for_reasoning(prompt, max_output_tokens, level)`.
//! 4. Returns the driver's text + real `TokenUsage`. The engine reads the
//!    `TokenUsage.total() > 0` branch (see `engine.rs`) and skips the
//!    chars/4 heuristic.

use crate::{FactReference, ReasoningError, ReasoningLevel, ReasoningLlm};
use async_trait::async_trait;
use openfang_types::message::TokenUsage;
use std::sync::Weak;

/// Capability the kernel exposes to the reasoning engine. The single
/// method takes the assembled prompt and a token budget; the kernel
/// chooses the model + temperature based on `level` and forwards to the
/// agent's existing `LlmDriver`. Implementations live in
/// `openfang-kernel::kernel`.
#[async_trait]
pub trait KernelLlm: Send + Sync + 'static {
    /// Run one LLM completion for a reasoning call. Returns the generated
    /// text and the driver's real `TokenUsage`.
    async fn complete_for_reasoning(
        &self,
        prompt: &str,
        max_output_tokens: u32,
        level: ReasoningLevel,
    ) -> Result<(String, TokenUsage), ReasoningError>;
}

/// Adapter wiring a `KernelLlm`-capable kernel into the reasoning engine's
/// `ReasoningLlm` seam.
pub struct KernelLlmAdapter<K: KernelLlm + ?Sized> {
    kernel: Weak<K>,
    max_input_tokens: u32,
    max_output_tokens: u32,
}

impl<K: KernelLlm + ?Sized> KernelLlmAdapter<K> {
    /// Build an adapter holding a weak reference to the kernel + the
    /// per-call token caps from `[reasoning]`.
    pub fn new(kernel: Weak<K>, max_input_tokens: u32, max_output_tokens: u32) -> Self {
        Self {
            kernel,
            max_input_tokens,
            max_output_tokens,
        }
    }
}

/// Build the prompt sent to the LLM. Depth-tuned via `level`:
/// `Minimal`/`Low` use a terse "summarize the facts" instruction; `Medium`
/// and above add explicit reasoning guidance.
fn build_prompt(query: &str, facts: &[FactReference], level: ReasoningLevel) -> String {
    use std::fmt::Write;
    let mut buf = String::with_capacity(query.len() + facts.len() * 80 + 256);
    match level {
        ReasoningLevel::Minimal | ReasoningLevel::Low => {
            buf.push_str(
                "You are answering a memory-recall question. Use ONLY the facts below; do not invent details.\n\n",
            );
        }
        ReasoningLevel::Medium => {
            buf.push_str(
                "Synthesize an answer using the facts below. If the facts conflict, mention the conflict. Cite by [source].\n\n",
            );
        }
        ReasoningLevel::High | ReasoningLevel::Max => {
            buf.push_str(
                "Reason carefully across the facts below. Consider context, contradictions, and timing. Cite by [source] and call out caveats explicitly.\n\n",
            );
        }
    }
    if facts.is_empty() {
        buf.push_str("Facts:\n  (none)\n\n");
    } else {
        buf.push_str("Facts:\n");
        for f in facts {
            let _ = writeln!(buf, "- [{}] {}", source_short(&f.source), f.content);
        }
        buf.push('\n');
    }
    let _ = writeln!(buf, "Question: {}", query);
    buf.push_str("Answer:");
    buf
}

/// Render a `FactSource` as a short tag for prompt citations. Stable
/// across runs (no random IDs, no map iteration order).
fn source_short(src: &crate::FactSource) -> String {
    match src {
        crate::FactSource::Memory { memory_id } => format!("mem:{}", short_id(memory_id)),
        crate::FactSource::Session {
            session_id,
            message_index,
        } => format!("sess:{}#{}", short_id(session_id), message_index),
        crate::FactSource::KnowledgeGraph { entity_id } => format!("kg:{}", short_id(entity_id)),
        crate::FactSource::StructuredKv { key } => format!("kv:{}", key),
    }
}

/// Take the first 8 chars of an opaque ID for the citation tag.
fn short_id(id: &str) -> String {
    if id.len() <= 8 {
        id.to_string()
    } else {
        id.chars().take(8).collect()
    }
}

/// UTF-8-safe truncate to ≤`max_bytes`. Walks back to the nearest char
/// boundary so multibyte codepoints never panic the slice.
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

#[async_trait]
impl<K: KernelLlm + ?Sized> ReasoningLlm for KernelLlmAdapter<K> {
    async fn synthesize(
        &self,
        query: &str,
        facts: &[FactReference],
        level: ReasoningLevel,
    ) -> Result<String, ReasoningError> {
        let (text, _usage) = self.synthesize_with_usage(query, facts, level).await?;
        Ok(text)
    }

    async fn synthesize_with_usage(
        &self,
        query: &str,
        facts: &[FactReference],
        level: ReasoningLevel,
    ) -> Result<(String, TokenUsage), ReasoningError> {
        let kernel = self.kernel.upgrade().ok_or_else(|| {
            ReasoningError::Llm("kernel dropped before reasoning LLM call".to_string())
        })?;
        let prompt = build_prompt(query, facts, level);
        // ~4 chars per token heuristic — pre-call truncation cap.
        let max_chars = (self.max_input_tokens as usize).saturating_mul(4);
        let prompt = safe_truncate(&prompt, max_chars);
        kernel
            .complete_for_reasoning(&prompt, self.max_output_tokens, level)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FactSource;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    struct CountingKernel {
        calls: AtomicU32,
        last_prompt: std::sync::Mutex<String>,
        last_max_out: std::sync::Mutex<u32>,
        last_level: std::sync::Mutex<Option<ReasoningLevel>>,
    }

    #[async_trait]
    impl KernelLlm for CountingKernel {
        async fn complete_for_reasoning(
            &self,
            prompt: &str,
            max_output_tokens: u32,
            level: ReasoningLevel,
        ) -> Result<(String, TokenUsage), ReasoningError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            *self.last_prompt.lock().unwrap() = prompt.to_string();
            *self.last_max_out.lock().unwrap() = max_output_tokens;
            *self.last_level.lock().unwrap() = Some(level);
            Ok((
                "synth".to_string(),
                TokenUsage {
                    input_tokens: 123,
                    output_tokens: 45,
                },
            ))
        }
    }

    fn fact(src: FactSource, content: &str) -> FactReference {
        FactReference {
            source: src,
            content: content.to_string(),
            relevance: 0.9,
            timestamp: None,
        }
    }

    #[tokio::test]
    async fn adapter_returns_real_token_usage() {
        let k = Arc::new(CountingKernel {
            calls: AtomicU32::new(0),
            last_prompt: std::sync::Mutex::new(String::new()),
            last_max_out: std::sync::Mutex::new(0),
            last_level: std::sync::Mutex::new(None),
        });
        let adapter = KernelLlmAdapter::new(Arc::downgrade(&k), 4_000, 2_000);
        let (txt, usage) = adapter
            .synthesize_with_usage(
                "What is rust?",
                &[fact(
                    FactSource::StructuredKv {
                        key: "rust".to_string(),
                    },
                    "love it",
                )],
                ReasoningLevel::Medium,
            )
            .await
            .unwrap();
        assert_eq!(txt, "synth");
        assert_eq!(usage.input_tokens, 123);
        assert_eq!(usage.output_tokens, 45);
        assert_eq!(k.calls.load(Ordering::SeqCst), 1);
        assert_eq!(*k.last_max_out.lock().unwrap(), 2_000);
        assert_eq!(
            *k.last_level.lock().unwrap(),
            Some(ReasoningLevel::Medium)
        );
        let prompt = k.last_prompt.lock().unwrap().clone();
        assert!(prompt.contains("Question: What is rust?"));
        assert!(prompt.contains("[kv:rust] love it"));
    }

    #[tokio::test]
    async fn adapter_errors_when_kernel_dropped() {
        let k = Arc::new(CountingKernel {
            calls: AtomicU32::new(0),
            last_prompt: std::sync::Mutex::new(String::new()),
            last_max_out: std::sync::Mutex::new(0),
            last_level: std::sync::Mutex::new(None),
        });
        let weak = Arc::downgrade(&k);
        drop(k);
        let adapter = KernelLlmAdapter::<CountingKernel>::new(weak, 4_000, 2_000);
        let err = adapter
            .synthesize_with_usage("q", &[], ReasoningLevel::Minimal)
            .await
            .unwrap_err();
        assert!(matches!(err, ReasoningError::Llm(_)));
        assert!(err.to_string().to_lowercase().contains("kernel"));
    }

    #[tokio::test]
    async fn adapter_truncates_prompt_to_max_input_tokens_times_four() {
        // 10 input tokens × 4 chars = 40 byte cap. The prompt header alone
        // is already > 40 bytes, so the truncated prompt should be exactly
        // 40 bytes (or shorter at a char boundary).
        let k = Arc::new(CountingKernel {
            calls: AtomicU32::new(0),
            last_prompt: std::sync::Mutex::new(String::new()),
            last_max_out: std::sync::Mutex::new(0),
            last_level: std::sync::Mutex::new(None),
        });
        let adapter = KernelLlmAdapter::new(Arc::downgrade(&k), 10, 100);
        let _ = adapter
            .synthesize_with_usage(
                "What is rust?",
                &[fact(
                    FactSource::StructuredKv {
                        key: "rust".to_string(),
                    },
                    "the answer is 42 and the answer is 42",
                )],
                ReasoningLevel::Medium,
            )
            .await
            .unwrap();
        let prompt = k.last_prompt.lock().unwrap().clone();
        assert!(
            prompt.len() <= 40,
            "expected truncation to <= 40 bytes, got {}",
            prompt.len()
        );
    }

    #[test]
    fn safe_truncate_respects_char_boundaries() {
        let s = "héllo";
        assert_eq!(safe_truncate(s, 2), "h");
        assert_eq!(safe_truncate(s, 99), "héllo");
    }
}

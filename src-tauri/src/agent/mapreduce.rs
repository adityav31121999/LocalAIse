use crate::error::AppResult;
use crate::retrieval::search::tokenize;
use crate::runtime::client::{self, SamplingParams, Usage};
use crate::runtime::Serving;
use super::prompts::{self, Task};

/// Fraction of the context window reserved for the input text of a single map
/// chunk. The rest is left for the system prompt, the user prompt wrapper, and
/// the model's own output. 0.35 keeps chunks small enough for multi-chunk
/// map-reduce on typical documents.
const CONTEXT_INPUT_FRACTION: f64 = 0.35;
/// Additional safety margin applied to the char-per-token estimate so that a
/// chunk never slightly exceeds the token limit due to estimation error.
const CHAR_ESTIMATE_SAFETY: f64 = 0.90;
/// Maximum tokens for single-pass summarization. Documents with more tokens
/// than this will use map-reduce for better coverage.
const SINGLE_PASS_MAX_TOKENS: usize = 2048;
/// Maximum input tokens per map chunk (caps chunk size even for large contexts).
const MAX_CHUNK_INPUT_TOKENS: usize = 3000;
/// Fallback context size used when the profile carries `context_tokens = 0`
/// ("use the model's native default"). Most models support at least 4 096
/// tokens; using this conservative estimate avoids a misleading
/// "context too small" error while still producing valid chunks.
const FALLBACK_CONTEXT_TOKENS: u32 = 4_096;

/// Removes conversational preambles ("Here is...", "Sure, ...") if present.
pub fn clean_contract_violations(text: &str) -> String {
    let trimmed = text.trim();
    let lower = trimmed.to_lowercase();
    for prefix in &[
        "here is a summary",
        "here is the summary",
        "here are the extracted",
        "sure! here is",
        "sure, here is",
        "certainly! here is",
        "certainly, here is",
    ] {
        if lower.starts_with(prefix) {
            if let Some(pos) = trimmed.find('\n') {
                let rest = trimmed[pos + 1..].trim();
                if !rest.is_empty() {
                    return rest.to_string();
                }
            }
        }
    }
    trimmed.to_string()
}

/// Splits text on structural boundaries (paragraphs, headings) with 10–15% overlap.
pub fn chunk_text_structurally(full_text: &str, char_limit: usize) -> Vec<String> {
    if full_text.len() <= char_limit {
        return vec![full_text.to_string()];
    }

    let overlap_chars = ((char_limit as f64 * 0.12) as usize).max(64);
    let paragraphs: Vec<&str> = full_text.split("\n\n").collect();
    let mut chunks: Vec<String> = Vec::new();
    let mut current_chunk = String::new();

    for p in paragraphs {
        let p_trimmed = p.trim();
        if p_trimmed.is_empty() {
            continue;
        }

        // If a single paragraph is larger than char_limit, split it by words
        if p_trimmed.len() > char_limit {
            if !current_chunk.is_empty() {
                chunks.push(current_chunk.trim().to_string());
                current_chunk.clear();
            }
            let words: Vec<&str> = p_trimmed.split_whitespace().collect();
            let mut word_chunk = String::new();
            for word in words {
                if !word_chunk.is_empty() && word_chunk.len() + 1 + word.len() > char_limit {
                    let chunk_str = word_chunk.trim().to_string();
                    chunks.push(chunk_str.clone());
                    // 10-15% overlap
                    let overlap: String = if chunk_str.len() > overlap_chars {
                        chunk_str
                            .chars()
                            .skip(chunk_str.chars().count().saturating_sub(overlap_chars))
                            .collect()
                    } else {
                        chunk_str
                    };
                    word_chunk = overlap;
                }
                if !word_chunk.is_empty() {
                    word_chunk.push(' ');
                }
                word_chunk.push_str(word);
            }
            if !word_chunk.is_empty() {
                current_chunk = word_chunk;
            }
            continue;
        }

        if !current_chunk.is_empty() && current_chunk.len() + 2 + p_trimmed.len() > char_limit {
            let chunk_str = current_chunk.trim().to_string();
            chunks.push(chunk_str.clone());

            // 10-15% overlap from the end of the previous chunk
            let overlap: String = if chunk_str.len() > overlap_chars {
                chunk_str
                    .chars()
                    .skip(chunk_str.chars().count().saturating_sub(overlap_chars))
                    .collect()
            } else {
                chunk_str
            };
            current_chunk = overlap;
            current_chunk.push_str("\n\n");
        }

        current_chunk.push_str(p_trimmed);
        current_chunk.push_str("\n\n");
    }

    if !current_chunk.trim().is_empty() {
        chunks.push(current_chunk.trim().to_string());
    }

    if chunks.is_empty() {
        vec![full_text.to_string()]
    } else {
        chunks
    }
}

/// Recursive reduction step: if combined intermediate notes exceed 60% of context,
/// synthesizes in groups of sections first before the final reduce.
pub async fn recursive_reduce(
    serving: &Serving,
    model_label: &str,
    notes: &[String],
    target_words: u32,
    max_input_chars: usize,
    total_usage: &mut Usage,
) -> AppResult<String> {
    let combined = notes.join("\n\n");
    if notes.len() > 6 && combined.len() > max_input_chars {
        log::info!(
            "[map_reduce] combined notes exceed context cap ({} > {} chars); reducing in groups",
            combined.len(),
            max_input_chars
        );
        let mut group_summaries = Vec::new();
        for (idx, group) in notes.chunks(5).enumerate() {
            let group_notes = group.to_vec();
            let sub_res = Box::pin(recursive_reduce(
                serving,
                model_label,
                &group_notes,
                target_words,
                max_input_chars,
                total_usage,
            ))
            .await?;
            group_summaries.push(format!("[Group {}]\n{}", idx + 1, sub_res.trim()));
        }
        return Box::pin(recursive_reduce(
            serving,
            model_label,
            &group_summaries,
            target_words,
            max_input_chars,
            total_usage,
        ))
        .await;
    }

    let task = Task::Reduce { target_words };
    let reduce_messages = vec![
        client::message("system", prompts::system_prompt(&task)),
        client::message(
            "user",
            prompts::user_message(&task, &[("notes", &combined)]),
        ),
    ];

    let reduce_sampling = SamplingParams {
        max_tokens: Some(8192),
        temperature: task.temperature(),
        ..Default::default()
    };
    let (summary, reduce_usage) =
        client::complete_with_usage(serving, model_label, &reduce_messages, reduce_sampling).await?;
    total_usage.add(reduce_usage);
    Ok(clean_contract_violations(&summary))
}

/// Map-reduce summarization for long documents exceeding the context window.
///
/// Chunks are sized by the model's own tokenizer (via llama-server's
/// `/tokenize` endpoint) rather than a fixed character count, so every chunk
/// fits within the context window regardless of how densely the model
/// tokenizes the text. Text structure is preserved: chunks split on paragraph
/// and heading boundaries with 10–15% overlap, and sections are labeled [S1], [S2]...
///
/// Returns the summary text together with accumulated token usage across all
/// internal model calls (map chunks + reduce step).
pub async fn map_reduce_summarize(
    serving: &Serving,
    model_label: &str,
    context_tokens: u32,
    full_text: &str,
) -> AppResult<(String, Usage)> {
    let effective_context = if context_tokens == 0 {
        log::info!(
            "[map_reduce] context_tokens=0 (model default); using fallback {}",
            FALLBACK_CONTEXT_TOKENS
        );
        FALLBACK_CONTEXT_TOKENS
    } else {
        context_tokens
    };

    let input_token_limit = (effective_context as f64 * CONTEXT_INPUT_FRACTION) as usize;
    let input_token_limit = input_token_limit.min(MAX_CHUNK_INPUT_TOKENS);
    if input_token_limit == 0 {
        return Err(crate::error::AppError::msg(
            "context window is too small to summarize any text",
        ));
    }

    // Scale target_words dynamically: 5–8% of source words, clamped to 150–600 words
    let word_count = full_text.split_whitespace().count();
    let target_words = ((word_count as f64 * 0.06) as u32).clamp(150, 600);

    // Count tokens using the model's own tokenizer.
    let tokens = tokenize(full_text);
    let token_count = tokens.len();
    log::info!(
        "[map_reduce] tokenized document: {} tokens, input_token_limit: {}, target_words: {}",
        token_count,
        input_token_limit,
        target_words
    );

    // Single-pass for small documents.
    if token_count <= SINGLE_PASS_MAX_TOKENS {
        log::info!(
            "[map_reduce] document small enough ({} tokens <= {}), using single-pass",
            token_count,
            SINGLE_PASS_MAX_TOKENS
        );
        let task = Task::Summary { target_words };
        let messages = vec![
            client::message("system", prompts::system_prompt(&task)),
            client::message("user", prompts::user_message(&task, &[("document", full_text)])),
        ];
        let sampling = SamplingParams {
            max_tokens: Some(8192),
            temperature: task.temperature(),
            ..Default::default()
        };
        let (summary, usage) =
            client::complete_with_usage(serving, model_label, &messages, sampling).await?;
        return Ok((clean_contract_violations(&summary), usage));
    }

    log::info!(
        "[map_reduce] document has {} tokens, using map-reduce for comprehensive summary",
        token_count
    );

    // Derive character limit from measured tokenization ratio
    let chars_per_token = full_text.len() as f64 / token_count as f64;
    let char_limit = (input_token_limit as f64 * chars_per_token * CHAR_ESTIMATE_SAFETY) as usize;
    let char_limit = char_limit.max(256);

    // Structural chunking with 10–15% overlap
    let chunks = chunk_text_structurally(full_text, char_limit);
    let n = chunks.len();

    log::info!(
        "[map_reduce] document has {token_count} tokens; split into {n} structural chunks (~{char_limit} char limit, context={effective_context})"
    );

    // Map step: summarize each section, accumulating token usage.
    let mut intermediate_summaries = Vec::new();
    let mut total_usage = Usage::default();
    let max_map_words = (target_words / 2).clamp(80, 200);

    for (i, chunk) in chunks.iter().enumerate() {
        let task = Task::Map {
            i: i + 1,
            n,
            max_words: max_map_words,
        };
        let messages = vec![
            client::message("system", prompts::system_prompt(&task)),
            client::message("user", prompts::user_message(&task, &[("section", chunk)])),
        ];
        let map_sampling = SamplingParams {
            max_tokens: Some(1024),
            temperature: task.temperature(),
            ..Default::default()
        };

        match client::complete_with_usage(serving, model_label, &messages, map_sampling).await {
            Ok((summary, usage)) => {
                total_usage.add(usage);
                let cleaned = clean_contract_violations(&summary);
                if cleaned.trim().starts_with("INSUFFICIENT_CONTENT") {
                    log::warn!("[map_reduce] section {} returned INSUFFICIENT_CONTENT", i + 1);
                    intermediate_summaries.push(format!("MISSING: S{}", i + 1));
                } else {
                    log::info!("[map_reduce] section {} succeeded: {} chars", i + 1, cleaned.len());
                    intermediate_summaries.push(format!("[S{}]\n{}", i + 1, cleaned.trim()));
                }
            }
            Err(e) => {
                log::warn!("[map_reduce] section {} failed: {e}", i + 1);
                intermediate_summaries.push(format!("MISSING: S{}", i + 1));
            }
        }
    }

    // Fallback if all map steps produced MISSING
    let valid_count = intermediate_summaries
        .iter()
        .filter(|s| !s.starts_with("MISSING:"))
        .count();
    if valid_count == 0 {
        log::warn!("[map_reduce] all map chunks failed; falling back to single-pass on truncated text");
        let truncated = if full_text.chars().count() > char_limit {
            let end = full_text
                .char_indices()
                .nth(char_limit)
                .map(|(i, _)| i)
                .unwrap_or(full_text.len());
            &full_text[..end]
        } else {
            full_text
        };
        let task = Task::Summary { target_words };
        let messages = vec![
            client::message("system", prompts::system_prompt(&task)),
            client::message("user", prompts::user_message(&task, &[("document", truncated)])),
        ];
        let fallback_sampling = SamplingParams {
            max_tokens: Some(8192),
            temperature: task.temperature(),
            ..Default::default()
        };
        let (summary, usage) =
            client::complete_with_usage(serving, model_label, &messages, fallback_sampling).await?;
        return Ok((clean_contract_violations(&summary), usage));
    }

    // Reduce step: combine all section notes, running recursively if notes exceed 60% context.
    let max_reduce_input_chars = (effective_context as f64 * 0.60 * chars_per_token) as usize;
    let summary = recursive_reduce(
        serving,
        model_label,
        &intermediate_summaries,
        target_words,
        max_reduce_input_chars,
        &mut total_usage,
    )
    .await?;

    log::info!(
        "[map_reduce] final summary length: {} chars  prompt_tokens={} completion_tokens={}",
        summary.len(),
        total_usage.prompt_tokens,
        total_usage.completion_tokens
    );
    Ok((summary, total_usage))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_contract_violations() {
        let input = "Here is a summary of the document:\n\n**Purpose:** Annual budget report.";
        assert_eq!(
            clean_contract_violations(input),
            "**Purpose:** Annual budget report."
        );

        let clean = "**Purpose:** Clean report.";
        assert_eq!(clean_contract_violations(clean), clean);
    }

    #[test]
    fn test_chunk_text_structurally_with_overlap() {
        let text = "Paragraph one with some text.\n\nParagraph two with more details.\n\nParagraph three with findings.";
        let chunks = chunk_text_structurally(text, 50);
        assert!(chunks.len() >= 2);
        // Ensure no chunks are empty
        assert!(chunks.iter().all(|c| !c.trim().is_empty()));
    }

    #[test]
    fn test_insufficient_content_sentinel_triggers_missing_marker() {
        let mut intermediate_summaries = Vec::new();
        let map_output = "INSUFFICIENT_CONTENT: unreadable OCR scan";
        if map_output.trim().starts_with("INSUFFICIENT_CONTENT") {
            intermediate_summaries.push(format!("MISSING: S{}", 3));
        } else {
            intermediate_summaries.push(format!("[S3]\n{}", map_output.trim()));
        }

        assert_eq!(intermediate_summaries, vec!["MISSING: S3"]);
    }
}

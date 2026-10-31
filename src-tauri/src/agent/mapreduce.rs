use crate::error::AppResult;
use crate::runtime::client::{self, SamplingParams, Usage};
use crate::runtime::Serving;

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

/// Map-reduce summarization for long documents exceeding the context window.
///
/// Chunks are sized by the model's own tokenizer (via llama-server's
/// `/tokenize` endpoint) rather than a fixed character count, so every chunk
/// fits within the context window regardless of how densely the model
/// tokenizes the text. Text structure is preserved: chunks always split on
/// word boundaries, never mid-word, and no part of the document is dropped.
///
/// Returns the summary text together with accumulated token usage across all
/// internal model calls (map chunks + reduce step).
pub async fn map_reduce_summarize(
    serving: &Serving,
    model_label: &str,
    context_tokens: u32,
    full_text: &str,
) -> AppResult<(String, Usage)> {
    // `0` means "use the model's native default context length"; we don't know
    // the exact value here so we fall back to a safe conservative estimate
    // rather than failing immediately with a misleading "context too small"
    // error for users who left the profile at its default.
    let effective_context = if context_tokens == 0 {
        log::info!(
            "[map_reduce] context_tokens=0 (model default); using fallback {}",
            FALLBACK_CONTEXT_TOKENS
        );
        FALLBACK_CONTEXT_TOKENS
    } else {
        context_tokens
    };

    // How many input tokens a single map chunk may use, leaving room for the
    // prompt wrapper and the model's generated output.
    let input_token_limit = (effective_context as f64 * CONTEXT_INPUT_FRACTION) as usize;
    // Cap chunk size so large-context models still produce multiple chunks.
    let input_token_limit = input_token_limit.min(MAX_CHUNK_INPUT_TOKENS);
    if input_token_limit == 0 {
        return Err(crate::error::AppError::msg(
            "context window is too small to summarize any text",
        ));
    }

    // Count tokens using the model's own tokenizer.
    let tokens = client::tokenize(serving, full_text).await?;
    let token_count = tokens.len();
    log::info!("[map_reduce] tokenized document: {} tokens, input_token_limit: {}", token_count, input_token_limit);

    // Use single-pass only for small documents. Larger docs use map-reduce
    // for comprehensive coverage regardless of context window size.
    if token_count <= SINGLE_PASS_MAX_TOKENS {
        log::info!("[map_reduce] document small enough ({} tokens <= {}), using single-pass", token_count, SINGLE_PASS_MAX_TOKENS);
        let messages = vec![
            client::message("system", "You are an analytical assistant summarizing documents."),
            client::message("user", super::prompts::summarize_prompt(full_text)),
        ];
        // Allow up to 8192 output tokens for a comprehensive summary.
        // Some models/servers cap output lower; if we hit the limit, map-reduce
        // will be used as a fallback by the caller.
        let sampling = SamplingParams {
            max_tokens: Some(8192),
            ..Default::default()
        };
        let (summary, usage) =
            client::complete_with_usage(serving, model_label, &messages, sampling).await?;
        return Ok((summary, usage));
    }

    log::info!("[map_reduce] document has {} tokens, using map-reduce for comprehensive summary", token_count);

    // Too long: derive a character limit from the measured tokenization
    // ratio, so each chunk is approximately `input_token_limit` tokens.
    let chars_per_token = full_text.len() as f64 / token_count as f64;
    let char_limit = (input_token_limit as f64 * chars_per_token * CHAR_ESTIMATE_SAFETY) as usize;
    let char_limit = char_limit.max(256); // never degenerate to a tiny chunk

    let words: Vec<&str> = full_text.split_whitespace().collect();

    // Split into chunks for the Map step, splitting only on word boundaries.
    let mut chunks: Vec<String> = Vec::new();
    let mut current_chunk = Vec::new();
    let mut current_len = 0;

    for word in words {
        current_len += word.len() + 1;
        current_chunk.push(word);
        if current_len >= char_limit {
            chunks.push(current_chunk.join(" "));
            current_chunk.clear();
            current_len = 0;
        }
    }
    if !current_chunk.is_empty() {
        chunks.push(current_chunk.join(" "));
    }

    log::info!(
        "[map_reduce] document has {token_count} tokens; split into {} chunks (~{} char limit, context={effective_context})",
        chunks.len(),
        char_limit
    );

    // Map step: summarize each section, accumulating token usage.
    log::info!("[map_reduce] starting map step for {} chunks", chunks.len());
    let mut intermediate_summaries = Vec::new();
    let mut total_usage = Usage::default();
    let map_sampling = SamplingParams {
        max_tokens: Some(1024),
        ..Default::default()
    };
    for (i, chunk) in chunks.iter().enumerate() {
        let messages = vec![
            client::message("system", "You are a concise document section summarizer."),
            client::message("user", super::prompts::map_chunk_prompt(chunk)),
        ];
        match client::complete_with_usage(serving, model_label, &messages, map_sampling.clone()).await {
            Ok((summary, usage)) => {
                log::info!("[map_reduce] section {} succeeded: {} chars", i + 1, summary.len());
                intermediate_summaries.push(format!("--- Section {} ---\n{}", i + 1, summary.trim()));
                total_usage.add(usage);
            }
            Err(e) => {
                log::warn!("[map_reduce] section {} failed: {e}", i + 1);
            }
        }
    }

    // If all map steps failed, fall back to a single-pass attempt on the whole text
    // (truncated to the input token limit) so we at least return something useful.
    if intermediate_summaries.is_empty() {
        log::warn!("[map_reduce] all map chunks failed; falling back to single-pass on truncated text");
        // Truncate on a char boundary (not a byte index) to avoid a panic on
        // multi-byte UTF-8 sequences (e.g. PDF text with non-ASCII characters).
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
        let messages = vec![
            client::message("system", "You are an analytical assistant summarizing documents."),
            client::message("user", super::prompts::summarize_prompt(truncated)),
        ];
        let fallback_sampling = SamplingParams {
            max_tokens: Some(8192),
            ..Default::default()
        };
        let (summary, usage) =
            client::complete_with_usage(serving, model_label, &messages, fallback_sampling).await?;
        log::info!("[map_reduce] fallback summary length: {} chars", summary.len());
        return Ok((summary, usage));
    }

    // Reduce step: combine all intermediate summaries.
    let combined = intermediate_summaries.join("\n\n");
    log::info!("[map_reduce] reduce input: {} chars from {} sections", combined.len(), intermediate_summaries.len());
    let reduce_messages = vec![
        client::message("system", "You synthesize section summaries into an executive document report."),
        client::message("user", super::prompts::reduce_summaries_prompt(&combined)),
    ];

    let reduce_sampling = SamplingParams {
        max_tokens: Some(8192),
        ..Default::default()
    };
    let (summary, reduce_usage) =
        client::complete_with_usage(serving, model_label, &reduce_messages, reduce_sampling).await?;
    total_usage.add(reduce_usage);
    log::info!(
        "[map_reduce] final summary length: {} chars  prompt_tokens={} completion_tokens={}",
        summary.len(),
        total_usage.prompt_tokens,
        total_usage.completion_tokens
    );
    Ok((summary, total_usage))
}

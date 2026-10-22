use crate::error::AppResult;
use crate::runtime::client;
use crate::runtime::Serving;

const CHUNK_CHAR_LIMIT: usize = 3_000;

/// Map-reduce summarization for long documents exceeding the context window.
pub async fn map_reduce_summarize(
    serving: &Serving,
    model_label: &str,
    full_text: &str,
) -> AppResult<String> {
    let words: Vec<&str> = full_text.split_whitespace().collect();

    // If small enough, summarize directly
    if full_text.len() <= CHUNK_CHAR_LIMIT {
        let messages = vec![
            client::message("system", "You are an analytical assistant summarizing documents."),
            client::message("user", super::prompts::summarize_prompt(full_text)),
        ];
        return client::complete(serving, model_label, &messages).await;
    }

    // Split into chunks for Map step
    let mut chunks: Vec<String> = Vec::new();
    let mut current_chunk = Vec::new();
    let mut current_len = 0;

    for word in words {
        current_len += word.len() + 1;
        current_chunk.push(word);
        if current_len >= CHUNK_CHAR_LIMIT {
            chunks.push(current_chunk.join(" "));
            current_chunk.clear();
            current_len = 0;
        }
    }
    if !current_chunk.is_empty() {
        chunks.push(current_chunk.join(" "));
    }

    // Map step: summarize each section
    let mut intermediate_summaries = Vec::new();
    for (i, chunk) in chunks.iter().enumerate() {
        let messages = vec![
            client::message("system", "You are a concise document section summarizer."),
            client::message("user", super::prompts::map_chunk_prompt(chunk)),
        ];
        if let Ok(summary) = client::complete(serving, model_label, &messages).await {
            intermediate_summaries.push(format!("--- Section {} ---\n{}", i + 1, summary.trim()));
        }
    }

    // Reduce step: combine all intermediate summaries
    let combined = intermediate_summaries.join("\n\n");
    let reduce_messages = vec![
        client::message("system", "You synthesize section summaries into an executive document report."),
        client::message("user", super::prompts::reduce_summaries_prompt(&combined)),
    ];

    client::complete(serving, model_label, &reduce_messages).await
}

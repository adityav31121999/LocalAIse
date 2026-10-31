//! Prompt templates for document analysis operations.

pub fn summarize_prompt(document_text: &str) -> String {
    format!(
        "Summarize the following document clearly and completely, highlighting main ideas, key takeaways, and conclusions in full:\n\n{document_text}"
    )
}

pub fn extract_tables_prompt(document_text: &str) -> String {
    format!(
        "Extract all tabular data, structured lists, and numerical metrics from the following text into clean Markdown tables:\n\n{document_text}"
    )
}

pub fn compare_documents_prompt(doc_a_name: &str, doc_a_text: &str, doc_b_name: &str, doc_b_text: &str) -> String {
    format!(
        "Compare the following two documents in detail. Highlight similarities, key differences, contrasting claims, and changes:\n\n\
        Document 1: {doc_a_name}\n\
        {doc_a_text}\n\n\
        Document 2: {doc_b_name}\n\
        {doc_b_text}"
    )
}

pub fn flag_inconsistencies_prompt(document_text: &str) -> String {
    format!(
        "Analyze the following document for logical inconsistencies, contradictory claims, numerical discrepancies, or ambiguous statements. List any flagged items with explanations:\n\n{document_text}"
    )
}

pub fn map_chunk_prompt(chunk_text: &str) -> String {
    format!(
        "Summarize this section of a larger document concisely in 1-2 paragraphs, capturing key facts, methods, and findings:\n\n{chunk_text}"
    )
}

pub fn reduce_summaries_prompt(combined_summaries: &str) -> String {
    format!(
        "Synthesize these section summaries into one cohesive, comprehensive, well-structured document summary. Thoroughly cover main ideas, key takeaways, and conclusions in full without cutting off:\n\n{combined_summaries}"
    )
}

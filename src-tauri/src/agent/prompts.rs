//! Prompt templates for document analysis operations.

pub fn summarize_prompt(document_text: &str) -> String {
    format!(
        "Summarize the following document clearly, highlighting main ideas, key takeaways, and conclusions:\n\n{document_text}"
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
        "Summarize this section of a larger document, capturing essential facts, names, numbers, and decisions:\n\n{chunk_text}"
    )
}

pub fn reduce_summaries_prompt(combined_summaries: &str) -> String {
    format!(
        "Synthesize these section summaries into one cohesive, comprehensive, well-structured document summary:\n\n{combined_summaries}"
    )
}

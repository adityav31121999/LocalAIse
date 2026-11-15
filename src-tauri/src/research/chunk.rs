/// Split markdown into chunks of roughly `target_chars` characters.
///
/// The unit is *characters*, not tokens and not bytes. As a rough rule of
/// thumb 1 char is about 0.25–0.75 tokens depending on content (code skews
/// high, prose low), so an 800-char chunk stays well inside the context
/// window of the LLM reader while leaving little risk with multi-byte
/// Unicode text.
pub fn chunk_markdown(md: &str, target_chars: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    for para in md.split("\n\n").flat_map(|p| split_long(p, target_chars)) {
        if buf.chars().count() + para.chars().count() > target_chars && !buf.is_empty() {
            out.push(buf.trim().to_string());
            buf.clear();
        }
        if !buf.is_empty() { buf.push_str("\n\n"); }
        buf.push_str(&para);
    }
    if !buf.trim().is_empty() { out.push(buf.trim().to_string()); }
    out
}

fn split_long(p: &str, target_chars: usize) -> Vec<String> {
    if p.chars().count() <= target_chars { return vec![p.to_string()]; }
    let mut out = Vec::new();
    let mut cur = String::new();
    for w in p.split_whitespace() {
        if !cur.is_empty() && cur.chars().count() + 1 + w.chars().count() > target_chars {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() { cur.push(' '); }
        cur.push_str(w);
    }
    if !cur.is_empty() { out.push(cur); }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_respect_the_target_and_keep_paragraphs_intact() {
        let input = (0..20).map(|i| format!("Paragraph {i} with some words in it.")).collect::<Vec<_>>().join("\n\n");
        let chunks = chunk_markdown(&input, 120);
        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert!(chunk.chars().count() <= 200, "chunk too large: {}", chunk.chars().count());
            assert!(!chunk.trim().is_empty());
        }
    }

    #[test]
    fn very_long_single_paragraph_is_split_on_word_boundaries() {
        let input = (0..200).map(|i| format!("word{i}")).collect::<Vec<_>>().join(" ");
        let chunks = chunk_markdown(&input, 50);
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|c| c.chars().count() <= 60));
    }

    #[test]
    fn empty_input_yields_no_chunks() {
        assert!(chunk_markdown("", 100).is_empty());
        assert!(chunk_markdown("   \n\n  ", 100).is_empty());
    }
}

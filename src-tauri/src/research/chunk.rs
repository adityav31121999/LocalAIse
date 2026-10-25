pub fn chunk_markdown(md: &str, target: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    for para in md.split("\n\n") {
        if buf.len() + para.len() > target && !buf.is_empty() {
            out.push(buf.trim().to_string());
            buf.clear();
        }
        if !buf.is_empty() { buf.push_str("\n\n"); }
        buf.push_str(para);
    }
    if !buf.trim().is_empty() { out.push(buf.trim().to_string()); }
    out
}
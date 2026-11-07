pub fn chunk_markdown(md: &str, target: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    for para in md.split("\n\n").flat_map(|p| split_long(p, target)) {
        if buf.len() + para.len() > target && !buf.is_empty() {
            out.push(buf.trim().to_string());
            buf.clear();
        }
        if !buf.is_empty() { buf.push_str("\n\n"); }
        buf.push_str(&para);
    }
    if !buf.trim().is_empty() { out.push(buf.trim().to_string()); }
    out
}

fn split_long(p: &str, target: usize) -> Vec<String> {
    if p.len() <= target { return vec![p.to_string()]; }
    let mut out = Vec::new();
    let mut cur = String::new();
    for w in p.split_whitespace() {
        if !cur.is_empty() && cur.len() + 1 + w.len() > target {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() { cur.push(' '); }
        cur.push_str(w);
    }
    if !cur.is_empty() { out.push(cur); }
    out
}
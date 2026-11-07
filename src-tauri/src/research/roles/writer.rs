use crate::research::fact::Fact;

pub fn write_section(title: &str, facts: &[&Fact]) -> String {
    let mut md = format!("## {title}\n\n");
    if facts.is_empty() {
        md.push_str("_No supported evidence found._\n");
    }
    for f in facts {
        md.push_str(&format!("- {} [{}]\n", f.claim, f.source_id));
    }
    md
}
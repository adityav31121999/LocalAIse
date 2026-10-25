pub fn write_section(title: &str, facts: &[String]) -> String {
    let mut md = format!("## {title}\n\n");
    for f in facts { md.push_str(&format!("- {f}\n")); }
    md
}
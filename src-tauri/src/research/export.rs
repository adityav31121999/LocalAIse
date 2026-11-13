use crate::research::report_writer::{WrittenSection, Citation};
use crate::research::store::Source;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::Datelike;

/// Citation style for bibliography formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CitationStyle {
    /// APA 7th edition
    Apa,
    /// IEEE
    Ieee,
    /// Chicago Author-Date
    Chicago,
    /// MLA 9th edition
    Mla,
    /// Harvard
    Harvard,
    /// Simple numeric (default)
    Numeric,
}

/// Export format for research output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    /// Markdown (default)
    Markdown,
    /// JSON with structured data
    Json,
    /// HTML
    Html,
    /// Notion-compatible blocks
    Notion,
    /// Plain text
    Text,
    /// CSL-JSON for Zotero/Mendeley
    CslJson,
}

/// Export options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportOptions {
    pub format: ExportFormat,
    pub citation_style: CitationStyle,
    pub include_methodology: bool,
    pub include_sources: bool,
    pub include_confidence: bool,
    pub include_limitations: bool,
    pub include_conflicts: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            format: ExportFormat::Markdown,
            citation_style: CitationStyle::Numeric,
            include_methodology: true,
            include_sources: true,
            include_confidence: true,
            include_limitations: true,
            include_conflicts: true,
        }
    }
}

/// Bibliography entry for citation styles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BibliographyEntry {
    pub citation_id: usize,
    pub source: Source,
    pub citation: Citation,
    pub formatted: String,
}

/// Generate bibliography in specified citation style.
pub fn generate_bibliography(
    sections: &[WrittenSection],
    sources: &[Source],
    style: CitationStyle,
) -> Vec<BibliographyEntry> {
    let mut cited: HashMap<i64, (usize, &Source, &Citation)> = HashMap::new();
    let mut next_id = 1;

    for section in sections {
        for citation in &section.citations {
            if !cited.contains_key(&citation.source_id) {
                if let Some(source) = sources.iter().find(|s| s.id == citation.source_id) {
                    cited.insert(citation.source_id, (next_id, source, citation));
                    next_id += 1;
                }
            }
        }
    }

    let mut entries: Vec<_> = cited.values().map(|(id, source, citation)| {
        let formatted = format_citation(id, source, citation, style);
        BibliographyEntry {
            citation_id: *id,
            source: (*source).clone(),
            citation: (*citation).clone(),
            formatted,
        }
    }).collect();

    // Sort by citation_id
    entries.sort_by_key(|e| e.citation_id);
    entries
}

/// Format a single citation according to style.
fn format_citation(id: &usize, source: &Source, citation: &Citation, style: CitationStyle) -> String {
    let title = if source.title.is_empty() { "Untitled" } else { &source.title };
    let url = &source.url;
    let year = extract_year(source);

    match style {
        CitationStyle::Apa => {
            format!("{}. ({}). {}. {}", 
                format_authors_apa(source), 
                year, 
                title, 
                url
            )
        }
        CitationStyle::Ieee => {
            format!("[{}] {}, \"{}\", {}", id, format_authors_ieee(source), title, url)
        }
        CitationStyle::Chicago => {
            format!("{}. {}. {}. {}", 
                format_authors_chicago(source), 
                title, 
                year, 
                url
            )
        }
        CitationStyle::Mla => {
            format!("{}. \"{}\". {}, {}.", 
                format_authors_mla(source), 
                title, 
                year, 
                url
            )
        }
        CitationStyle::Harvard => {
            format!("{} ({}) \"{}\". Available at: {}", 
                format_authors_harvard(source), 
                year, 
                title, 
                url
            )
        }
        CitationStyle::Numeric => {
            format!("[{}] {}", id, citation.source_url)
        }
    }
}

fn extract_year(source: &Source) -> String {
    // Try to extract year from URL or title
    let text = format!("{} {}", source.url, source.title);
    let re = regex::Regex::new(r"\b(19|20)\d{2}\b").unwrap();
    re.find(&text).map(|m| m.as_str().to_string()).unwrap_or_else(|| "n.d.".to_string())
}

fn format_authors_apa(source: &Source) -> String {
    // Try to extract author from URL domain
    let domain = extract_domain(&source.url);
    format!("{}.", domain)
}

fn format_authors_ieee(source: &Source) -> String {
    let domain = extract_domain(&source.url);
    format!("{}", domain)
}

fn format_authors_chicago(source: &Source) -> String {
    let domain = extract_domain(&source.url);
    format!("{}", domain)
}

fn format_authors_mla(source: &Source) -> String {
    let domain = extract_domain(&source.url);
    format!("{}", domain)
}

fn format_authors_harvard(source: &Source) -> String {
    let domain = extract_domain(&source.url);
    format!("{}", domain)
}

fn extract_domain(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "Unknown".to_string())
}

/// Export research report to JSON.
pub fn export_json(
    sections: &[WrittenSection],
    sources: &[Source],
    options: &ExportOptions,
) -> serde_json::Value {
    let bibliography = if options.include_sources {
        generate_bibliography(sections, sources, options.citation_style)
    } else {
        vec![]
    };

    serde_json::json!({
        "sections": sections.iter().map(|s| {
            serde_json::json!({
                "title": s.title,
                "content": s.content,
                "confidence": s.confidence,
                "citations": s.citations.iter().map(|c| {
                    serde_json::json!({
                        "fact_id": c.fact_id,
                        "source_id": c.source_id,
                        "source_url": c.source_url,
                        "source_title": c.source_title,
                        "quote": c.quote,
                        "verification": c.verification
                    })
                }).collect::<Vec<_>>(),
                "conflicting_evidence": s.conflicting_evidence,
                "limitations": s.limitations
            })
        }).collect::<Vec<_>>(),
        "bibliography": bibliography.iter().map(|b| {
            serde_json::json!({
                "citation_id": b.citation_id,
                "source": {
                    "id": b.source.id,
                    "url": b.source.url,
                    "title": b.source.title,
                    "quality_score": b.source.quality_score
                },
                "formatted": b.formatted
            })
        }).collect::<Vec<_>>(),
        "metadata": {
            "citation_style": format!("{:?}", options.citation_style),
            "total_sections": sections.len(),
            "total_citations": bibliography.len()
        }
    })
}

/// Export research report to Notion-compatible blocks.
pub fn export_notion(
    sections: &[WrittenSection],
    sources: &[Source],
    options: &ExportOptions,
) -> Vec<serde_json::Value> {
    let mut blocks = Vec::new();

    // Title
    blocks.push(notion_heading("Research Report", 1));
    
    // Executive summary
    blocks.push(notion_paragraph(
        &format!("This report covers {} sections with {} total citations.", 
            sections.len(), 
            sections.iter().map(|s| s.citations.len()).sum::<usize>())
    ));

    for section in sections {
        // Section heading
        blocks.push(notion_heading(&section.title, 2));
        
        // Confidence badge
        if options.include_confidence {
            let confidence_label = match section.confidence {
                c if c >= 0.7 => "🟢 High",
                c if c >= 0.4 => "🟡 Medium",
                _ => "🔴 Low",
            };
            blocks.push(notion_callout(&format!("Confidence: {} ({:.0}%)", confidence_label, section.confidence * 100.0)));
        }

        // Content (split into paragraphs)
        for para in section.content.split("\n\n") {
            let para = para.trim();
            if para.is_empty() { continue; }
            if para.starts_with("## ") {
                blocks.push(notion_heading(para.trim_start_matches("## ").trim(), 3));
            } else if para.starts_with("### ") {
                blocks.push(notion_heading(para.trim_start_matches("### ").trim(), 4));
            } else if para.starts_with("- ") || para.starts_with("* ") {
                blocks.push(notion_bullet(para.trim_start_matches("- ").trim_start_matches("* ").trim()));
            } else {
                blocks.push(notion_paragraph(para));
            }
        }

        // Conflicting evidence
        if options.include_conflicts {
            if let Some(conflicting) = &section.conflicting_evidence {
                blocks.push(notion_callout(conflicting));
            }
        }

        // Limitations
        if options.include_limitations {
            if let Some(limitations) = &section.limitations {
                blocks.push(notion_callout(limitations));
            }
        }
    }

    // Bibliography
    if options.include_sources {
        blocks.push(notion_heading("Sources", 2));
        let bibliography = generate_bibliography(sections, sources, options.citation_style);
        for entry in bibliography {
            blocks.push(notion_paragraph(&format!("{}. {}", entry.citation_id, entry.formatted)));
        }
    }

    blocks
}

fn notion_heading(text: &str, _level: u8) -> serde_json::Value {
    serde_json::json!({
        "object": "block",
        "type": "heading_1",
        "heading_1": {
            "rich_text": [{ "type": "text", "text": { "content": text } }]
        }
    })
}

fn notion_paragraph(text: &str) -> serde_json::Value {
    serde_json::json!({
        "object": "block",
        "type": "paragraph",
        "paragraph": {
            "rich_text": [{ "type": "text", "text": { "content": text } }]
        }
    })
}

fn notion_bullet(text: &str) -> serde_json::Value {
    serde_json::json!({
        "object": "block",
        "type": "bulleted_list_item",
        "bulleted_list_item": {
            "rich_text": [{ "type": "text", "text": { "content": text } }]
        }
    })
}

fn notion_callout(text: &str) -> serde_json::Value {
    serde_json::json!({
        "object": "block",
        "type": "callout",
        "callout": {
            "rich_text": [{ "type": "text", "text": { "content": text } }],
            "icon": { "emoji": "⚠️" }
        }
    })
}

/// Export to CSL-JSON for reference managers.
pub fn export_csl_json(
    sections: &[WrittenSection],
    sources: &[Source],
) -> serde_json::Value {
    let bibliography = generate_bibliography(sections, sources, CitationStyle::Apa);
    
    let items: Vec<serde_json::Value> = bibliography.iter().map(|b| {
        serde_json::json!({
            "id": format!("source-{}", b.source.id),
            "type": "webpage",
            "title": b.source.title,
            "URL": b.source.url,
            "author": [{ "literal": extract_domain(&b.source.url) }],
            "issued": { "date-parts": [[extract_year(&b.source).parse::<i32>().unwrap_or(2024)]] },
            "accessed": { "date-parts": [[chrono::Utc::now().year() as i32, chrono::Utc::now().month() as i32, chrono::Utc::now().day() as i32]] }
        })
    }).collect();

    serde_json::json!(items)
}

/// Export to plain text.
pub fn export_text(
    sections: &[WrittenSection],
    sources: &[Source],
    options: &ExportOptions,
) -> String {
    let mut out = String::new();
    out.push_str("RESEARCH REPORT\n");
    out.push_str("===============\n\n");

    for section in sections {
        out.push_str(&format!("{} (Confidence: {:.0}%)\n", section.title, section.confidence * 100.0));
        out.push_str(&section.content);
        out.push('\n');
        
        if options.include_conflicts {
            if let Some(conflicting) = &section.conflicting_evidence {
                out.push_str(&format!("\nCONFLICTING EVIDENCE:\n{}\n", conflicting));
            }
        }
        
        if options.include_limitations {
            if let Some(limitations) = &section.limitations {
                out.push_str(&format!("\nLIMITATIONS:\n{}\n", limitations));
            }
        }
    }

    if options.include_sources {
        out.push_str("\n\nSOURCES\n");
        out.push_str("=======\n\n");
        let bibliography = generate_bibliography(sections, sources, options.citation_style);
        for entry in bibliography {
            out.push_str(&format!("{}. {}\n", entry.citation_id, entry.formatted));
        }
    }

    out
}

/// Main export function.
pub fn export_report(
    sections: &[WrittenSection],
    sources: &[Source],
    options: ExportOptions,
) -> Result<String, crate::research::error::ResearchError> {
    match options.format {
        ExportFormat::Markdown => Ok(crate::research::report_writer::write_report(sections.to_vec(), sources)),
        ExportFormat::Json => Ok(serde_json::to_string_pretty(&export_json(sections, sources, &options))
            .map_err(|e| crate::research::error::ResearchError::Internal(e.to_string()))?),
        ExportFormat::Html => Ok(export_html(sections, sources, &options)),
        ExportFormat::Notion => Ok(serde_json::to_string_pretty(&export_notion(sections, sources, &options))
            .map_err(|e| crate::research::error::ResearchError::Internal(e.to_string()))?),
        ExportFormat::Text => Ok(export_text(sections, sources, &options)),
        ExportFormat::CslJson => Ok(serde_json::to_string_pretty(&export_csl_json(sections, sources))
            .map_err(|e| crate::research::error::ResearchError::Internal(e.to_string()))?),
    }
}

/// Export to HTML.
pub fn export_html(
    sections: &[WrittenSection],
    sources: &[Source],
    options: &ExportOptions,
) -> String {
    let mut html = String::new();
    html.push_str("<!DOCTYPE html>\n<html>\n<head>\n");
    html.push_str("<meta charset=\"utf-8\">\n");
    html.push_str("<title>Research Report</title>\n");
    html.push_str("<style>\n");
    html.push_str(r#"
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; max-width: 800px; margin: 0 auto; padding: 2rem; line-height: 1.6; }
        h1, h2, h3 { color: #1a1a2e; }
        .confidence { display: inline-block; padding: 0.25rem 0.5rem; border-radius: 0.25rem; font-size: 0.875rem; font-weight: 600; }
        .confidence-high { background: #d1fae5; color: #065f46; }
        .confidence-medium { background: #fef3c7; color: #92400e; }
        .confidence-low { background: #fee2e2; color: #991b1b; }
        .citation { color: #2563eb; text-decoration: none; }
        .callout { border-left: 4px solid #f59e0b; padding: 1rem; margin: 1rem 0; background: #fffbeb; }
        .sources { margin-top: 3rem; padding-top: 2rem; border-top: 1px solid #e5e7eb; }
        blockquote { border-left: 4px solid #3b82f6; padding-left: 1rem; margin: 1rem 0; color: #4b5563; }
    "#);
    html.push_str("</style>\n</head>\n<body>\n");
    
    html.push_str("<h1>Research Report</h1>\n");
    
    for section in sections {
        let confidence_class = if section.confidence >= 0.7 { "confidence-high" } else if section.confidence >= 0.4 { "confidence-medium" } else { "confidence-low" };
        html.push_str(&format!("<h2>{} <span class=\"confidence {}\">{:.0}%</span></h2>\n", section.title, confidence_class, section.confidence * 100.0));
        
        // Convert markdown to HTML (simplified)
        let content_html = markdown_to_html(&section.content);
        html.push_str(&content_html);
        
        if options.include_conflicts {
            if let Some(conflicting) = &section.conflicting_evidence {
                html.push_str(&format!("<div class=\"callout\"><strong>Conflicting Evidence:</strong> {}</div>\n", conflicting));
            }
        }
        
        if options.include_limitations {
            if let Some(limitations) = &section.limitations {
                html.push_str(&format!("<div class=\"callout\"><strong>Limitations:</strong> {}</div>\n", limitations));
            }
        }
    }

    if options.include_sources {
        html.push_str("<div class=\"sources\"><h2>Sources</h2>\n<ol>\n");
        let bibliography = generate_bibliography(sections, sources, options.citation_style);
        for entry in bibliography {
            html.push_str(&format!("<li>{}</li>\n", entry.formatted));
        }
        html.push_str("</ol></div>\n");
    }

    html.push_str("</body>\n</html>");
    html
}

fn markdown_to_html(md: &str) -> String {
    let mut html = String::new();
    for line in md.lines() {
        let line = line.trim();
        if line.is_empty() {
            html.push_str("<p></p>\n");
        } else if line.starts_with("## ") {
            html.push_str(&format!("<h2>{}</h2>\n", &line[3..]));
        } else if line.starts_with("### ") {
            html.push_str(&format!("<h3>{}</h3>\n", &line[4..]));
        } else if line.starts_with("- ") || line.starts_with("* ") {
            html.push_str(&format!("<li>{}</li>\n", &line[2..]));
        } else if line.starts_with("[") && line.contains("](") {
            // Inline citation - keep as-is
            html.push_str(&format!("<p>{}</p>\n", line));
        } else {
            html.push_str(&format!("<p>{}</p>\n", line));
        }
    }
    html
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research::report_writer::{WrittenSection, Citation};
    use crate::research::roles::verify::VerificationLabel;
    use crate::research::store::Source;

    fn make_section(title: &str) -> WrittenSection {
        WrittenSection {
            title: title.into(),
            content: format!("## {}\n\nSome content here.", title),
            citations: vec![],
            confidence: 0.8,
            conflicting_evidence: None,
            limitations: None,
        }
    }

    fn make_source(id: i64, url: &str) -> Source {
        Source { id, url: url.into(), title: "Test".into(), text: "Test".into(), quality_score: 0.8 }
    }

    #[test]
    fn test_export_json() {
        let sections = vec![make_section("Test")];
        let sources = vec![make_source(1, "https://example.com")];
        let options = ExportOptions::default();
        
        let json = export_json(&sections, &sources, &options);
        assert!(json["sections"].is_array());
        assert_eq!(json["sections"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_bibliography_generation() {
        let sections = vec![make_section("Test")];
        let sources = vec![make_source(1, "https://example.com/page")];
        
        // Add a citation to the section
        let mut section = sections[0].clone();
        section.citations = vec![Citation {
            fact_id: 0,
            source_id: 1,
            source_url: "https://example.com/page".into(),
            source_title: "Test".into(),
            quote: "Test quote".into(),
            verification: VerificationLabel::Supported,
        }];
        let sections = vec![section];
        
        let bib = generate_bibliography(&sections, &sources, CitationStyle::Apa);
        assert_eq!(bib.len(), 1);
        assert!(bib[0].formatted.contains("example.com"));
    }
}
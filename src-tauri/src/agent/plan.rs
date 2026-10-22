//! Research Plan model and generation.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchStep {
    pub step_number: usize,
    pub title: String,
    pub query: String,
    pub source_type: String, // "local" | "web"
    #[serde(default)]
    pub completed: bool,
    #[serde(default)]
    pub findings: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchPlan {
    pub topic: String,
    pub max_steps: usize,
    pub steps: Vec<ResearchStep>,
    pub status: String, // "planned" | "approved" | "running" | "completed"
}

pub fn create_starter_plan(topic: &str, internet_enabled: bool) -> ResearchPlan {
    let mut steps = Vec::new();

    steps.push(ResearchStep {
        step_number: 1,
        title: "Search indexed local sources".to_string(),
        query: topic.to_string(),
        source_type: "local".to_string(),
        completed: false,
        findings: None,
    });

    if internet_enabled {
        steps.push(ResearchStep {
            step_number: 2,
            title: "Web discovery and cross-verification".to_string(),
            query: format!("{topic} overview details"),
            source_type: "web".to_string(),
            completed: false,
            findings: None,
        });
    }

    steps.push(ResearchStep {
        step_number: steps.len() + 1,
        title: "Synthesize findings into cited report".to_string(),
        query: "synthesize".to_string(),
        source_type: "local".to_string(),
        completed: false,
        findings: None,
    });

    ResearchPlan {
        topic: topic.to_string(),
        max_steps: 5,
        steps,
        status: "planned".to_string(),
    }
}

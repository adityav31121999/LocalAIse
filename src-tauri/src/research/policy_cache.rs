use crate::research::error::ResearchError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Domain allow/block list configuration per project.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DomainPolicy {
    pub allow_list: Vec<String>,
    pub block_list: Vec<String>,
    pub default_action: DomainAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DomainAction {
    #[default]
    Allow,
    Block,
}

impl DomainPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn check(&self, url: &str) -> Result<(), ResearchError> {
        let domain = extract_domain(url);
        
        // Check block list first
        for blocked in &self.block_list {
            if domain_matches(&domain, blocked) {
                return Err(ResearchError::PolicyBlocked {
                    url: url.to_string(),
                    reason: format!("Domain '{}' is blocked by project policy", domain),
                });
            }
        }

        // If allow list is configured, check it
        if !self.allow_list.is_empty() {
            let mut allowed = false;
            for allowed_domain in &self.allow_list {
                if domain_matches(&domain, allowed_domain) {
                    allowed = true;
                    break;
                }
            }
            if !allowed {
                return Err(ResearchError::PolicyBlocked {
                    url: url.to_string(),
                    reason: format!("Domain '{}' is not in project allow list", domain),
                });
            }
        }

        Ok(())
    }
}

fn domain_matches(domain: &str, pattern: &str) -> bool {
    // Support wildcards: *.example.com matches sub.example.com
    if pattern.starts_with("*.") {
        let suffix = &pattern[2..];
        domain == suffix || domain.ends_with(&format!(".{}", suffix))
    } else {
        domain == pattern
    }
}

fn extract_domain(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "unknown".to_string())
}

/// Per-role model configuration for routing different tasks to different models.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRouting {
    pub planner: ModelSpec,
    pub reader: ModelSpec,
    pub verifier: ModelSpec,
    pub writer: ModelSpec,
    pub gap_analyzer: ModelSpec,
    pub fallback: ModelSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSpec {
    pub model_label: String,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub cost_per_1m_input: f64,
    pub cost_per_1m_output: f64,
    pub capabilities: Vec<ModelCapability>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelCapability {
    JsonOutput,
    LongContext,
    Reasoning,
    FastInference,
    Multilingual,
}

impl Default for ModelRouting {
    fn default() -> Self {
        Self {
            planner: ModelSpec {
                model_label: "default".into(),
                temperature: Some(0.3),
                max_tokens: Some(1024),
                cost_per_1m_input: 0.0,
                cost_per_1m_output: 0.0,
                capabilities: vec![ModelCapability::JsonOutput],
            },
            reader: ModelSpec {
                model_label: "default".into(),
                temperature: Some(0.1),
                max_tokens: Some(1536),
                cost_per_1m_input: 0.0,
                cost_per_1m_output: 0.0,
                capabilities: vec![ModelCapability::JsonOutput, ModelCapability::LongContext],
            },
            verifier: ModelSpec {
                model_label: "default".into(),
                temperature: Some(0.0),
                max_tokens: Some(512),
                cost_per_1m_input: 0.0,
                cost_per_1m_output: 0.0,
                capabilities: vec![ModelCapability::JsonOutput, ModelCapability::Reasoning],
            },
            writer: ModelSpec {
                model_label: "default".into(),
                temperature: Some(0.2),
                max_tokens: Some(2048),
                cost_per_1m_input: 0.0,
                cost_per_1m_output: 0.0,
                capabilities: vec![ModelCapability::LongContext],
            },
            gap_analyzer: ModelSpec {
                model_label: "default".into(),
                temperature: Some(0.2),
                max_tokens: Some(2048),
                cost_per_1m_input: 0.0,
                cost_per_1m_output: 0.0,
                capabilities: vec![ModelCapability::JsonOutput, ModelCapability::Reasoning],
            },
            fallback: ModelSpec {
                model_label: "default".into(),
                temperature: Some(0.2),
                max_tokens: Some(1024),
                cost_per_1m_input: 0.0,
                cost_per_1m_output: 0.0,
                capabilities: vec![],
            },
        }
    }
}

/// Cross-run cache for sources and facts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchCache {
    /// Cached sources by URL hash
    pub sources: HashMap<String, CachedSource>,
    /// Cached facts by claim hash (multiple facts per claim)
    pub facts: HashMap<String, Vec<CachedFact>>,
    /// Cache metadata
    pub metadata: CacheMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedSource {
    pub url: String,
    pub title: String,
    pub text: String,
    pub quality_score: f32,
    pub domain_type: crate::research::search::SourceType,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
    pub content_hash: String,
    pub embedding: Option<Vec<f32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedFact {
    pub claim: String,
    pub quote: String,
    pub source_url: String,
    pub verification_label: crate::research::roles::verify::VerificationLabel,
    pub confidence: f32,
    pub extracted_at: chrono::DateTime<chrono::Utc>,
    pub corroboration_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMetadata {
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_updated: chrono::DateTime<chrono::Utc>,
    pub total_sources: usize,
    pub total_facts: usize,
    pub max_age_days: u32,
}

impl Default for ResearchCache {
    fn default() -> Self {
        Self {
            sources: HashMap::new(),
            facts: HashMap::new(),
            metadata: CacheMetadata {
                created_at: chrono::Utc::now(),
                last_updated: chrono::Utc::now(),
                total_sources: 0,
                total_facts: 0,
                max_age_days: 30,
            },
        }
    }
}

impl ResearchCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load(path: &Path) -> Result<Self, ResearchError> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let content = std::fs::read_to_string(path)
            .map_err(|e| ResearchError::Internal(format!("Failed to read cache: {}", e)))?;
        serde_json::from_str(&content)
            .map_err(|e| ResearchError::Internal(format!("Failed to parse cache: {}", e)))
    }

    pub fn save(&self, path: &Path) -> Result<(), ResearchError> {
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| ResearchError::Internal(format!("Failed to serialize cache: {}", e)))?;
        std::fs::write(path, content)
            .map_err(|e| ResearchError::Internal(format!("Failed to write cache: {}", e)))
    }

    /// Get a cached source if it exists and is fresh.
    pub fn get_source(&self, url: &str) -> Option<&CachedSource> {
        let key = hash_url(url);
        self.sources.get(&key).and_then(|cached| {
            let age = chrono::Utc::now().signed_duration_since(cached.fetched_at);
            if age.num_days() < self.metadata.max_age_days as i64 {
                Some(cached)
            } else {
                None
            }
        })
    }

    /// Store a source in the cache.
    pub fn put_source(&mut self, source: CachedSource) {
        let key = hash_url(&source.url);
        self.sources.insert(key, source);
        self.metadata.total_sources = self.sources.len();
        self.metadata.last_updated = chrono::Utc::now();
    }

    /// Get cached facts for a claim.
    pub fn get_facts(&self, claim: &str) -> &[CachedFact] {
        let key = hash_claim(claim);
        self.facts.get(&key).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Store a fact in the cache.
    pub fn put_fact(&mut self, fact: CachedFact) {
        let key = hash_claim(&fact.claim);
        self.facts.entry(key).or_default().push(fact);
        self.metadata.total_facts = self.facts.values().map(|v| v.len()).sum();
        self.metadata.last_updated = chrono::Utc::now();
    }

    /// Clean up stale entries.
    pub fn prune(&mut self) {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(self.metadata.max_age_days as i64);
        
        self.sources.retain(|_, v| v.fetched_at > cutoff);
        
        self.facts.retain(|_, facts| {
            facts.retain(|f| f.extracted_at > cutoff);
            !facts.is_empty()
        });
        
        self.metadata.total_sources = self.sources.len();
        self.metadata.total_facts = self.facts.values().map(|v| v.len()).sum();
    }
}

fn hash_url(url: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

fn hash_claim(claim: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let normalized = claim.to_lowercase().split_whitespace().filter(|w| w.len() > 2).collect::<Vec<_>>().join(" ");
    let mut hasher = DefaultHasher::new();
    normalized.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

/// Thread-safe cache wrapper.
pub type SharedCache = Arc<Mutex<ResearchCache>>;

/// Create a shared cache for a project.
pub fn create_project_cache(project_root: &Path) -> SharedCache {
    let cache_path = project_root.join("cache").join("research_cache.json");
    let cache = if cache_path.exists() {
        ResearchCache::load(&cache_path).unwrap_or_default()
    } else {
        ResearchCache::new()
    };
    Arc::new(Mutex::new(cache))
}

/// Save cache periodically.
pub fn save_cache_periodically(cache: SharedCache, project_root: &Path) {
    let cache_path = project_root.join("cache").join("research_cache.json");
    std::fs::create_dir_all(cache_path.parent().unwrap()).ok();
    if let Ok(cache) = cache.lock() {
        cache.save(&cache_path).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_domain_policy() {
        let mut policy = DomainPolicy::new();
        policy.allow_list = vec!["example.com".to_string(), "*.trusted.org".to_string()];
        policy.block_list = vec!["blocked.com".to_string()];
        
        assert!(policy.check("https://example.com/page").is_ok());
        assert!(policy.check("https://sub.trusted.org/page").is_ok());
        assert!(policy.check("https://blocked.com/page").is_err());
        assert!(policy.check("https://other.com/page").is_err()); // Not in allow list
    }

    #[test]
    fn test_domain_policy_no_allow_list() {
        let mut policy = DomainPolicy::new();
        policy.block_list = vec!["blocked.com".to_string()];
        
        assert!(policy.check("https://example.com/page").is_ok());
        assert!(policy.check("https://blocked.com/page").is_err());
    }

    #[test]
    fn test_cache_serialization() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cache.json");
        
        let mut cache = ResearchCache::new();
        cache.put_source(CachedSource {
            url: "https://example.com".into(),
            title: "Test".into(),
            text: "Content".into(),
            quality_score: 0.8,
            domain_type: crate::research::search::SourceType::Unknown,
            fetched_at: chrono::Utc::now(),
            content_hash: "abc123".into(),
            embedding: None,
        });
        
        cache.save(&path).unwrap();
        
        let loaded = ResearchCache::load(&path).unwrap();
        assert_eq!(loaded.sources.len(), 1);
    }

    #[test]
    fn test_cache_pruning() {
        let mut cache = ResearchCache::new();
        cache.metadata.max_age_days = 1;
        
        cache.put_source(CachedSource {
            url: "https://old.com".into(),
            title: "Old".into(),
            text: "Content".into(),
            quality_score: 0.5,
            domain_type: crate::research::search::SourceType::Unknown,
            fetched_at: chrono::Utc::now() - chrono::Duration::days(2),
            content_hash: "old".into(),
            embedding: None,
        });
        
        cache.put_source(CachedSource {
            url: "https://new.com".into(),
            title: "New".into(),
            text: "Content".into(),
            quality_score: 0.9,
            domain_type: crate::research::search::SourceType::Unknown,
            fetched_at: chrono::Utc::now(),
            content_hash: "new".into(),
            embedding: None,
        });
        
        cache.prune();
        assert_eq!(cache.sources.len(), 1);
        assert!(cache.sources.contains_key(&hash_url("https://new.com")));
    }
}
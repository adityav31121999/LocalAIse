pub mod error;
pub use error::{ResearchError, ResearchResult, BudgetKind};

pub mod llm;
pub use llm::{LlmClient, LlmMessage, LlmResponse, LlmParams, MockLlmClient, mock_json_response, parse_llm_json};

pub mod llm_adapter;
pub use llm_adapter::RuntimeLlmClient;

pub mod eval;
pub use eval::{EvalQuestion, ExpectedFact, EvalResult, EvalReport, EvalSummary, run_eval, EvalRunOutput, EvalSource, EvalFact};

pub mod search;
pub use search::{SearchProvider, SearchResult, FallbackSearch, canonicalize_url, dedupe_results, SourceType, SourceScore, CorroborationCluster, CorroboratedFact, cluster_claims, classify_domain};

pub mod search_providers;
pub use search_providers::{DuckDuckGoSearch, BraveSearch, TavilySearch, SearXNGSearch};

pub mod vector_store;
pub use vector_store::{VectorStore, Source as VectorSource, Chunk};

pub mod budget;
pub use budget::{Budgets, BudgetPreset, GapAnalysis, GapStatus, check_gaps, should_stop_adaptive, all_subquestions_answered};

pub mod report_writer;
pub use report_writer::{WrittenSection, Citation, write_report_section, write_report};

pub mod export;
pub use export::{CitationStyle, ExportFormat, ExportOptions, export_report, export_json, export_notion, export_csl_json, export_text, export_html, generate_bibliography};

pub mod policy_cache;
pub use policy_cache::{DomainPolicy, DomainAction, ModelRouting, ModelSpec, ModelCapability, ResearchCache, CachedSource, CachedFact, CacheMetadata, SharedCache, create_project_cache, save_cache_periodically};

pub mod events;
pub use events::ResearchEvent;

// Minimal phase-1 scaffold. Search/fetch/store/roles land here incrementally.
pub mod orchestrator;
pub mod fact;
pub mod request;
pub mod net;
pub mod local;
pub mod store;
pub mod store_sqlite;
pub mod fetch;
pub mod chunk;
pub mod roles;
pub mod rank;
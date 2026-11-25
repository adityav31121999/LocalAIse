//! Modular agent & tool execution harness.
//!
//! Phase 1 establishes the trait-based [`ToolDefinition`] abstraction,
//! [`ToolRegistry`], and 5-stage [`ToolPipeline`] (lookup, schema validation,
//! permission gate, before/after hooks, execution).

pub mod agent;
pub mod builtin;
pub mod checkpoint;
pub mod events;
pub mod mcp;
pub mod permissions;
pub mod pipeline;
pub mod provider;
pub mod registry;
pub mod subagent;
pub mod tool;
pub mod tool_search;

pub use agent::{load_project_instructions, AgentProfile, AgentProfileSummary, AgentRegistry};
pub use checkpoint::{CheckpointHook, CheckpointManager, CheckpointManifest, RollbackSummary};
pub use events::{HarnessEvent, HarnessEventStore, StoredHarnessEvent};
pub use permissions::{
    resolve_request, ApprovalDecision, InteractivePermissionGate, PermissionPolicy,
    PermissionState, RuntimeRequest,
};
pub use pipeline::{AuditLogHook, PermissionGate, ToolHook, ToolPipeline, TruncateOutputHook};
pub use provider::{
    list_providers, LlamaServerAdapter, ProviderAdapter, ProviderCapabilities, ProviderDescriptor,
};
pub use registry::ToolRegistry;
pub use tool::{ToolContext, ToolDefinition, ToolMeta};

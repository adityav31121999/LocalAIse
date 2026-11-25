//! Ordered registry of [`ToolDefinition`]s.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use serde_json::Value;

use super::builtin::register_builtins;
use super::tool::ToolDefinition;
use super::tool_search::{
    ToolSearchCandidate, ToolSearchTool, CORE_DEFERRED_TOOLS, DEFERRED_TOOL_THRESHOLD,
};

/// Stores tools in deterministic registration order with O(1) lookup by name.
#[derive(Default, Clone)]
pub struct ToolRegistry {
    tools: Vec<Arc<dyn ToolDefinition>>,
    index: HashMap<String, usize>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a registry populated with LocalAIse's built-in workspace,
    /// personal, and web tools.
    pub fn with_builtins() -> Self {
        let mut registry = Self::new();
        register_builtins(&mut registry);
        registry
    }

    /// Registers a tool; replaces any existing tool with the same `id`.
    pub fn register(&mut self, tool: Arc<dyn ToolDefinition>) {
        let id = tool.meta().id.to_string();
        if let Some(&pos) = self.index.get(&id) {
            self.tools[pos] = tool;
        } else {
            let pos = self.tools.len();
            self.tools.push(tool);
            self.index.insert(id, pos);
        }
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn ToolDefinition>> {
        self.index.get(name).map(|&pos| Arc::clone(&self.tools[pos]))
    }

    /// Returns OpenAI-compatible tool specifications filtered by internet
    /// availability and an optional tool-name allowlist.
    pub fn openai_specs(&self, internet_enabled: bool, allowlist: Option<&[&str]>) -> Vec<Value> {
        self.tools
            .iter()
            .filter(|tool| {
                let meta = tool.meta();
                if meta.requires_internet && !internet_enabled {
                    return false;
                }
                match allowlist {
                    Some(allowed) => allowed.contains(&meta.id),
                    None => true,
                }
            })
            .map(|tool| tool.to_openai_spec())
            .collect()
    }

    /// Registers `tool_search` when eligible tools exceed [`DEFERRED_TOOL_THRESHOLD`]
    /// and returns the shared unlocked-tools set.
    pub fn enable_deferred_search(
        &mut self,
        internet_enabled: bool,
        allowlist: Option<&[&str]>,
    ) -> Option<Arc<Mutex<HashSet<String>>>> {
        let candidates: Vec<ToolSearchCandidate> = self
            .tools
            .iter()
            .filter(|tool| {
                let meta = tool.meta();
                if meta.requires_internet && !internet_enabled {
                    return false;
                }
                match allowlist {
                    Some(allowed) => allowed.contains(&meta.id),
                    None => true,
                }
            })
            .map(|tool| {
                let m = tool.meta();
                ToolSearchCandidate {
                    id: m.id.to_string(),
                    description: m.description.to_string(),
                }
            })
            .collect();

        if candidates.len() <= DEFERRED_TOOL_THRESHOLD {
            return None;
        }

        let unlocked = Arc::new(Mutex::new(HashSet::new()));
        self.register(Arc::new(ToolSearchTool::new(
            candidates,
            Arc::clone(&unlocked),
        )));
        Some(unlocked)
    }

    /// Returns OpenAI-compatible specs, deferring non-core tools until
    /// unlocked by `tool_search` when `unlocked` is `Some(...)`.
    pub fn openai_specs_for_session(
        &self,
        internet_enabled: bool,
        allowlist: Option<&[&str]>,
        unlocked: Option<&Mutex<HashSet<String>>>,
    ) -> Vec<Value> {
        let unlocked_set = unlocked
            .and_then(|m| m.lock().ok())
            .map(|g| g.clone());

        self.tools
            .iter()
            .filter(|tool| {
                let meta = tool.meta();
                if meta.requires_internet && !internet_enabled {
                    return false;
                }
                if meta.id != "tool_search" {
                    if let Some(allowed) = allowlist {
                        if !allowed.contains(&meta.id) {
                            return false;
                        }
                    }
                }
                match &unlocked_set {
                    Some(set) => CORE_DEFERRED_TOOLS.contains(&meta.id) || set.contains(meta.id),
                    None => true,
                }
            })
            .map(|tool| tool.to_openai_spec())
            .collect()
    }
}

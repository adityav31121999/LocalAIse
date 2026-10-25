pub mod events;
pub use events::ResearchEvent;

// Minimal phase-1 scaffold. Search/fetch/store/roles land here incrementally.
pub mod orchestrator;
pub mod store;
pub mod store_sqlite;
pub mod fetch;
pub mod chunk;
pub mod roles;
pub mod rank;
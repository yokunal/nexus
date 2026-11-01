use crate::memory::{DistilledMemory, MemoryError, MemorySearchResult, MemoryStore};

pub fn execute_search_memory(
    store: &MemoryStore,
    query: &str,
    limit: usize,
) -> Result<Vec<MemorySearchResult>, MemoryError> {
    store.search(query, limit)
}

pub fn execute_distill_memory(
    store: &MemoryStore,
    session_id: &str,
    topic: &str,
    content: &str,
) -> Result<DistilledMemory, MemoryError> {
    store.save_distillation(session_id, topic, content)
}

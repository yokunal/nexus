pub mod config;
pub mod handback;
pub mod runner;
pub mod worktree;

pub use config::{AgentConfig, AgentDiscovery, AgentRole, AgentSource};
pub use handback::{SubagentHandback, SubagentStatus};
pub use runner::{AgentRunner, AgentTask, AgentTopology, WorkspaceMode};
pub use worktree::GitWorktreeManager;

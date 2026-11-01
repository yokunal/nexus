pub mod events;
pub mod git;
pub mod gstack;
pub mod provider;
pub mod rust_specialist;
pub mod sandbox;
pub mod session;
pub mod tools;

pub use events::NexusEvent;
pub use git::{GitSnapshotManager, TurnSnapshot};
pub use gstack::{ArchitectureInvariants, GStackEngine, GStackReviewMode};
pub use rust_specialist::{BorrowTutor, CheckMode, CheckResult, RustCheckRunner, RustDiagnostic};
pub use sandbox::Sandbox;
pub use session::{Message, Session, SessionStore};
pub use tools::edit_file::{EditError, PatchReport, PatchTier};
pub use tools::{ToolCall, ToolManager, ToolResult};

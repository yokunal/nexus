pub mod detector;
pub mod diagnostic;
pub mod runner;

pub use detector::{PolyglotDetector, PolyglotLanguage};
pub use diagnostic::{PolyglotCheckResult, PolyglotDiagnostic};
pub use runner::{PolyglotDoctor, PolyglotError};

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PolyglotLanguage {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    C,
    Cpp,
    Go,
    Unknown,
}

impl std::fmt::Display for PolyglotLanguage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PolyglotLanguage::Rust => write!(f, "Rust"),
            PolyglotLanguage::Python => write!(f, "Python"),
            PolyglotLanguage::JavaScript => write!(f, "JavaScript"),
            PolyglotLanguage::TypeScript => write!(f, "TypeScript"),
            PolyglotLanguage::C => write!(f, "C"),
            PolyglotLanguage::Cpp => write!(f, "C++"),
            PolyglotLanguage::Go => write!(f, "Go"),
            PolyglotLanguage::Unknown => write!(f, "Unknown"),
        }
    }
}

pub struct PolyglotDetector;

impl PolyglotDetector {
    pub fn detect(dir: &Path) -> PolyglotLanguage {
        // 1. Rust manifest
        if dir.join("Cargo.toml").exists() {
            return PolyglotLanguage::Rust;
        }

        // 2. Python manifests or files
        if dir.join("pyproject.toml").exists()
            || dir.join("requirements.txt").exists()
            || dir.join("setup.py").exists()
            || dir.join("Pipfile").exists()
        {
            return PolyglotLanguage::Python;
        }

        // 3. JS / TS manifests
        if dir.join("package.json").exists() {
            if dir.join("tsconfig.json").exists() {
                return PolyglotLanguage::TypeScript;
            }
            return PolyglotLanguage::JavaScript;
        }

        // 4. Go manifest
        if dir.join("go.mod").exists() {
            return PolyglotLanguage::Go;
        }

        // 5. C / C++ build files
        if dir.join("CMakeLists.txt").exists() || dir.join("Makefile").exists() {
            if Self::has_extension(dir, &["cpp", "cc", "cxx", "hpp"]) {
                return PolyglotLanguage::Cpp;
            }
            return PolyglotLanguage::C;
        }

        // 6. Scan workspace files by extension
        if Self::has_extension(dir, &["rs"]) {
            return PolyglotLanguage::Rust;
        }
        if Self::has_extension(dir, &["py"]) {
            return PolyglotLanguage::Python;
        }
        if Self::has_extension(dir, &["ts", "tsx"]) {
            return PolyglotLanguage::TypeScript;
        }
        if Self::has_extension(dir, &["js", "mjs", "cjs"]) {
            return PolyglotLanguage::JavaScript;
        }
        if Self::has_extension(dir, &["cpp", "cc", "cxx", "hpp"]) {
            return PolyglotLanguage::Cpp;
        }
        if Self::has_extension(dir, &["c", "h"]) {
            return PolyglotLanguage::C;
        }
        if Self::has_extension(dir, &["go"]) {
            return PolyglotLanguage::Go;
        }

        PolyglotLanguage::Unknown
    }

    fn has_extension(dir: &Path, exts: &[&str]) -> bool {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                        if exts.iter().any(|&e| e.eq_ignore_ascii_case(ext)) {
                            return true;
                        }
                    }
                } else if p.is_dir() {
                    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if name != ".git" && name != "target" && name != "node_modules" && name != ".venv" && name != "__pycache__" {
                        if let Ok(sub_entries) = std::fs::read_dir(&p) {
                            for sub in sub_entries.flatten() {
                                if let Some(ext) = sub.path().extension().and_then(|e| e.to_str()) {
                                    if exts.iter().any(|&e| e.eq_ignore_ascii_case(ext)) {
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }
}

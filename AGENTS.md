# Nexus Architectural Invariants & Agent Guidelines

## Core Invariants
- **Interface-First Contract**: All public interfaces, traits, and error enums must be defined before implementing concrete logic.
- **Explicit Error Domains**: Domain error enums must be created with `thiserror`. Never invoke `.unwrap()` or `.expect()` in non-test production code paths.
- **Ambiguity Guard**: The 3-tier patch engine must immediately reject ambiguous matches (count > 1) to prevent silent file corruption.
- **Strict Path Containment**: Every file operation must resolve within the workspace boundary; path traversal (`../`, absolute escapes) must be blocked by the sandbox.
- **Turn-Level Git Snapshot**: In git workspaces, capture exactly 1 lightweight snapshot per agent turn before executing modifying tools.
- **Defense-in-Depth Quarantine**: Destructive shell patterns are blacklisted and environment variables scrubbed (`GITHUB_TOKEN`, `SSH_AUTH_SOCK`, `HTTP_PROXY`, API keys). The interactive user prompt remains the true security gate.
- **Lean Runtime Footprint**: Release binary must remain ultra-lean (<15MB), fast startup (<0.02s), and 0% CPU idle.

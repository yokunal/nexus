use crate::skills::{Skill, SkillError, SkillManager};
use std::path::Path;

pub fn execute_synthesize_skill(
    workspace: &Path,
    name: &str,
    description: &str,
    instructions: &str,
) -> Result<Skill, SkillError> {
    SkillManager::synthesize_skill(workspace, name, description, instructions)
}

pub fn execute_list_skills(workspace: &Path) -> Result<Vec<Skill>, SkillError> {
    SkillManager::load_all(workspace)
}

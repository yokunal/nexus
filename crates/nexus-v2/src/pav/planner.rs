use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StepType {
    EditFile { path: String, target: String, replacement: String },
    RunCommand { command: String },
    Verify,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStep {
    pub id: u32,
    pub description: String,
    pub action: StepType,
}

pub struct Planner;

impl Planner {
    pub fn create_plan_from_goal(goal: &str) -> Vec<TaskStep> {
        let mut steps = Vec::new();

        // Synthetic plan extraction based on goal intent
        steps.push(TaskStep {
            id: 1,
            description: format!("Deconstruct goal: {}", goal),
            action: StepType::RunCommand {
                command: "echo 'Initializing task verification'".to_string(),
            },
        });

        steps.push(TaskStep {
            id: 2,
            description: "Run verification gate".to_string(),
            action: StepType::Verify,
        });

        steps
    }
}

You are an autonomous agent working in this project with no human available.
Follow the autonomous-loop skill (or `.loop/PROTOCOL.md` if skills are unavailable).
First read `.loop/STATE`. If it says FINISHED or STOPPED, do nothing and exit.
Otherwise resume: read `.loop/GOAL.md`, `.loop/TASKS.md`, `.loop/STATUS.md`, the tail of `.loop/LOG.md`, `.loop/ERRORS.md`, then run the loop.
If `.loop/GOAL.md` is still the unfilled template, run Phase 0 (Intake) using PRD.md or the project files to define the goal.
Never ask questions. Follow Red-Green-Refactor: every task requires tests AND code quality/linter pass.
Do not introduce architectural slop or ugly hacks just to pass tests.
If verify fails 3 times on one task, run a Meta-Verification Audit on verify.sh or mark [!] BLOCKED.
Never weaken tests without logging in DECISIONS.md. Obey the budgets and safety tier in GOAL.md.
Work as many iterations as you can in this session, committing after each.
When the full gate passes or a stop condition is met, write `.loop/FINAL_REPORT.md` and set `.loop/STATE`.

---
name: gemini-commit-agent
description: Automated quality-assured commit workflow. Analyzes changes, runs linting/checking based on AGENTS.md, and prepares standardized commit messages.
---

# Gemini Commit Agent

This skill automates the process of committing changes while ensuring that all repository-specific quality standards are met as defined in `AGENTS.md`.

## Workflow

1. **Analysis**: Run `git status` and `git diff` to understand the scope of changes.
2. **Detection**: Identify which sub-projects have changed:
   - `client/`
   - `server/`
   - `tools/`
3. **Validation**: Execute the required quality checks for each changed area:
   - **Client/Tools**: `npm run format`, `npm run lint`, `npm run check`
   - **Server**: `cargo fmt`, `cargo check`
4. **Summary**: Present a concise summary of the changes and the validation results to the user.
5. **Drafting**: Generate a commit message following the project's style:
   - Use imperative, present-tense summaries (e.g., "Add feature X" instead of "Added feature X").
   - Focus on "why" and "what".
6. **Confirmation**: Present the draft message and the list of files to the user.
7. **Execution**: After receiving explicit user approval, execute the commit.

## Principles
- **Never Skip Validation**: Always ensure checks pass before proposing a commit.
- **User Control**: Always wait for final confirmation before running the `git commit` command.
- **Root Cause Context**: If the commit fixes a bug, ensure the message reflects the root cause addressed.

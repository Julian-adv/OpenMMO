---
name: codex-commit-agent
description: Run the repository commit workflow safely and consistently. Use when the user asks to commit changes, says work is complete, or requests save-point commits. This skill detects changed project areas, runs required quality checks (`npm run format/lint/check`, `cargo fmt/check`), summarizes results, drafts an imperative English commit message, and asks for explicit user confirmation before running `git commit`.
---

# Codex Commit Agent

## Overview

Validate changed areas, stage the intended files, and create a clean commit only after required checks pass.
Follow repository policy from `AGENTS.md`: always ask for explicit user confirmation before commit.

## Workflow

1. Inspect current changes.
- Run `git status --short`.
- Build the changed-file set from staged, unstaged, and untracked files.
- If no changes exist, stop and report that there is nothing to commit.

2. Detect which project areas need checks.
- If any file is under `client/`, run client checks in `client/`.
- If files are under `tools/<tool-name>/`, run checks in each changed `tools/<tool-name>/`.
- If any file is under `server/`, run server checks in `server/`.

3. Run quality checks.
- Preferred path: run `./.codex/skills/codex-commit-agent/scripts/validate.sh`.
- Equivalent manual checks:
  - `client/` and each changed `tools/<tool-name>/`: `npm run format`, `npm run lint`, `npm run check`
  - `server/`: `cargo fmt`, `cargo check`
- If a check fails, stop and report the failing command and actionable errors.

4. Review commit contents.
- Run `git status --short` again.
- Review `git diff --staged` (or `git diff` if nothing is staged yet).
- Stage intended files with `git add ...` (avoid staging unrelated changes).

5. Draft and confirm.
- Draft a concise English commit message in imperative present tense.
- Keep the title under 72 characters.
- Show the proposed message and staged files to the user.
- Ask for explicit approval before running `git commit`.

6. Commit.
- After user approval, run `git commit -m "<message>"`.
- If commit fails, report the exact failure and next action.

## Commit Message Rules

- Use English only.
- Start with a verb: `Add`, `Fix`, `Update`, `Refactor`, `Remove`, `Improve`.
- Describe what changed clearly and specifically.
- Keep it scoped to staged files.

## Failure Handling

- Never commit when required checks fail.
- Never skip user confirmation.
- If formatting updates files, include those changes in the reviewed/staged set before commit.

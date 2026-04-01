# Claude Development Guidelines

- **Commit Workflow**: Always use the `commit-agent` skill when committing changes to ensure code quality checks.

## Engineering Principles
- No fallbacks. Make it fail. So we can notice and fix it.
- **Root Cause Analysis**: When fixing bugs, do not just fix the symptoms; find and address the root cause.
- **Simplicity**: Favor simple, readable solutions over complex abstractions.

## Project Structure & Module Organization
- `client`: Svelte 5, Threlte (Three.js), Vite, TS
- `tools/glb-editor`: SvelteKit, Three.js, Vite, TS
- `shared`: Shared code between `client` and `server`, (Rust -> WASM)
- `server`: Rust, Tokio

## Architecture & Performance
- **Avoid `useTask`**: Do not use Threlte's `useTask` for update logic in components.
- **Game Loop**: Use the centralized game loop in `GameScene.svelte`. Expose an `update(deltaTime)` method in your components and call it from the main loop to ensure deterministic execution order and performance throttling.

## Coding Style & Naming Conventions
### General
- Use 2-space indentation.
- No semicolons (configured in Prettier).
- Use single quotes for strings.

### Client & Tools (TypeScript/Svelte)
- **Svelte**: Use Svelte 5 syntax. Add keys to `{#each}` blocks.
- **Types**: Use proper TypeScript types; avoid `any`.
- **Naming**: Components in `PascalCase.svelte`, logic files in `kebab-case.ts`.
- **Logic**: Use block scopes in switch cases to avoid lexical declaration errors.

### Server (Rust)
- **Version**: Rust 2021 edition.
- **Naming**: Prefer `snake_case` for files/modules and `CamelCase` for types.
- **Logging**: Use `tracing` crate for logs.

## Refactoring Policy
This project is in early development. Do NOT worry about backwards compatibility:
- When renaming or removing exports, find and update ALL usages across the codebase.
- Do not leave deprecated aliases, re-exports, or compatibility shims.
- Do not add `@deprecated` comments - just remove unused code directly.
- If something is no longer needed, delete it completely.

## Testing Guidelines
- **Client**: No test runner configured yet; prefer adding Vitest for unit tests under `client/src` with `*.test.ts`.
- **Server**: Add `#[test]` or `#[tokio::test]` in module files. Place broader tests in `server/src/[module]/tests.rs`.

## Commit & Pull Request Guidelines
- **User Confirmation Required**: ALWAYS obtain explicit confirmation before committing. Present the commit message and staged files.
- **Quality Check**: Before committing, run:
  - Client/Tools: `npm run format`, `npm run lint`, `npm run check`
  - Server: `cargo fmt`, `cargo check`
- **Messages**: Use imperative, present-tense summaries (e.g., "Add HeightmapTerrain component").

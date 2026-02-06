# Gemini Development Guidelines

## Code Quality Workflow

Before committing code changes, always run the appropriate check commands (`npm run check`, `npm run lint`, `npm run format`) to ensure code quality.

## Common Commands

### Client Directory Commands

```bash
cd client
npm run lint          # Check for linting errors
npm run lint:fix       # Automatically fix linting errors
npm run format         # Format code with Prettier
npm run format:check   # Check if code is formatted correctly
npm run check          # Run Svelte and TypeScript type checking
npm run dev            # Start development server
npm run build          # Build for production
```

## Code Style Guidelines

- No semicolons (configured in Prettier)
- Use single quotes for strings
- Use proper TypeScript types (avoid `any`)
- Add keys to Svelte `{#each}` blocks
- Use block scopes in switch cases to avoid lexical declaration errors

## Architecture & Performance

- **Avoid `useTask`**: Do not use Threlte's `useTask` for update logic.
- **Game Loop**: Use the centralized game loop in `GameScene.svelte`. Expose an `update(deltaTime)` method in your components and call it from the main loop to ensure deterministic execution order and performance throttling.

## Refactoring Policy

This project is in early development. Do NOT worry about backwards compatibility:
- When renaming or removing exports, find and update ALL usages across the codebase
- Do not leave deprecated aliases, re-exports, or compatibility shims
- Do not add `@deprecated` comments - just remove unused code directly
- If something is no longer needed, delete it completely

## Notes

- ESLint is configured to work with JavaScript, TypeScript, and Svelte files
- Prettier is set up to format code without semicolons
- Always run lint checks after making code changes to maintain code quality

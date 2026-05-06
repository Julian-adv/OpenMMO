---
name: open-localhost-game
description: Open the local BotdleField development game in the Codex in-app browser, log in with credentials from the local ignored .env.local file or user-provided credentials, select the default character, and press Start to enter the game. Use when the user asks Codex to connect to localhost:10004, log into BotdleField, choose the default character, start the game, or restore the browser to the in-game state.
---

# Open Localhost Game

## Overview

Use this skill to drive the Codex in-app browser through the local BotdleField entry flow. The expected path is: open `http://localhost:10004/`, log in with credentials from the repository-root `.env.local`, keep the default selected character unless the user requests another one, and press `Start`.

## Workflow

1. Use the `browser-use:browser` skill and the in-app browser runtime.
2. Navigate to `http://localhost:10004/` unless the current tab is already there and the user has in-progress state worth preserving.
3. Read credentials from repository-root `.env.local`:
   - `BOTDLE_ACCOUNT_NAME`
   - `BOTDLE_PASSWORD`
4. If `.env.local` or either value is missing, ask the user for credentials instead of guessing.
5. On the login screen, fill the account name and password from those values.
6. Click `Login`.
7. Confirm the character select screen appears. A successful login shows `Character Select` and the logged-in account label.
8. Do not change the character when the user wants the default character; it is already selected.
9. Click `Start`.
10. Confirm the game UI appears. Expected signals include `Chat`, `Combat`, `Inventory (I)`, `World Map (M)`, and sometimes a `Preparing world...` dialog.

## Browser Notes

Prefer stable Playwright locators based on the visible form:

```js
const fs = await import("node:fs/promises");
const envText = await fs.readFile(`${nodeRepl.cwd}/.env.local`, "utf-8");
const env = Object.fromEntries(
  envText
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line && !line.startsWith("#"))
    .map((line) => {
      const index = line.indexOf("=");
      return [line.slice(0, index), line.slice(index + 1)];
    })
);
const accountName = env.BOTDLE_ACCOUNT_NAME;
const password = env.BOTDLE_PASSWORD;
if (!accountName || !password) throw new Error("Missing BotdleField credentials in .env.local");

await tab.goto("http://localhost:10004/");
await tab.playwright.getByPlaceholder("Enter your account", { exact: true }).fill(accountName, { timeoutMs: 5000 });
await tab.playwright.getByPlaceholder("Enter password", { exact: true }).fill(password, { timeoutMs: 5000 });
await tab.playwright.getByRole("button", { name: "Login", exact: true }).click({ timeoutMs: 5000 });
await tab.playwright.getByRole("button", { name: "Start", exact: true }).click({ timeoutMs: 5000 });
```

After each click, take a fresh DOM snapshot when reporting success or deciding the next action. If the login form is not visible, inspect the current screen first: the browser may already be logged in, on character select, or inside the game.

## Reporting

Keep the response concise. Report the final screen and any visible confirmation such as `Character Select`, the account label without repeating passwords, or the in-game controls.

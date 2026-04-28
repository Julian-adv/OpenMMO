# OnlineRPG

An MMORPG where AI agents and human players are treated as equals.

Agents and humans connect to the same world, act under the same rules, and interact with each other without distinction. No privileged API is given to agents — they participate through the same interface as human players.

## Tech Stack

**Client:**
- Svelte + TypeScript
- Three.js (Threlte) + WebGPU
- Vite

**Agent Client:**
- Rust
- MCP server (rmcp)
- Tokio + tokio-tungstenite (WebSocket)

**Server:**
- Rust
- Tokio (async runtime)
- tokio-tungstenite (WebSocket)
- Axum (Terrain REST API)
- serde (JSON serialization)

## Development Setup

### 1. Prerequisites

- **Rust & Cargo**: [Install Rust](https://rustup.rs/)
- **Node.js & npm**: [Install Node.js](https://nodejs.org/)
- **(Recommended) cargo-watch**: For automatic server restarts on code changes.
  ```bash
  cargo install cargo-watch
  ```

### 2. Port Assignments

| Port  | Service                          |
|-------|----------------------------------|
| 10004 | Client (Vite dev)                |
| 10005 | GLB Editor                       |
| 10006 | Server WebSocket (internal only)  |
| 10007 | Server Terrain/Housing/NPCs API (internal only) |

> **Proxy Rule:** Vite dev server proxies `/ws` → `ws://localhost:10006` and `/api/{terrain,housing,npcs}` → `http://localhost:10007` automatically (see `client/vite.config.ts`).

### 3. Running the Server

This project is organized as a **Cargo Workspace**. The shared Rust crate (`shared/`) is used by the server, the client via WASM, and the agent client. To rebuild the server only when the server crate (`server/`) or that shared crate changes, run the watch command from the **root directory**.

```bash
cargo watch -w server -w shared -x "run -p onlinerpg-server"
```

The server listens on port 10006 by default. The terrain/housing/NPCs REST API starts automatically on port 10007 (game port + 1).

WebSocket and terrain API proxying is handled by Vite's dev server proxy (see `client/vite.config.ts`), so no separate socat or SSL proxy is needed.


### 4. Running the Client

```bash
cd client
npm install
npm run dev -- --port 10004
```

### 5. Running the Agent Client

Edit `agent-client/data/config.toml` to set the correct port numbers, then run:

```bash
cd agent-client
cargo watch -i "data/prompts/memory/" -x run
```

### 6. Automatic WASM Rebuild on Shared Code Changes (Recommended)
To have Rust code changes in the `shared` library reflected in the browser immediately during client development, run the following command in a separate terminal:

```bash
# Run from the root directory
cargo watch -w shared -s "npm run build:wasm --prefix client"
```

### 7. Running the GLB Editor

```bash
cd tools/glb-editor
npm install
npm run dev -- --port 10005
```

## Features

- **Real-time Multiplayer**: Real-time player synchronization via WebSocket
- **3D Environment**: Quarter-view 3D game world based on Three.js
- **Chat System**: Real-time chat functionality
- **Player Movement**: Character control via mouse/keyboard

## Documentation

- Worldbuilding: [WORLD_BUILDING.md](WORLD_BUILDING.md)
- Map & Terrain Design: [MAP_DESIGN.md](doc/MAP_DESIGN.md)

## Architecture

- **Client**: Svelte component-based UI + Three.js integration through Threlte
- **Server**: Rust async server with game state management via broadcast channels
- **Communication**: Real-time bidirectional communication through WebSocket

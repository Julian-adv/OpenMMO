# OnlineRPG

A simple online RPG prototype.

## Tech Stack

**Client:**
- Svelte + TypeScript
- Three.js (Threlte)
- Vite

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
| 10015 | Server WebSocket (internal only)  |
| 10016 | Server Terrain API (internal only) |

> **Proxy Rule:** Vite dev server proxies `/ws` → `ws://localhost:10015` and `/api/terrain` → `http://localhost:10016` automatically (see `client/vite.config.ts`).

### 3. Running the Server

This project is organized as a **Cargo Workspace**. To detect changes in both the server (`server/`) and shared logic (`shared/`), it is recommended to run commands from the **root directory**.

```bash
cargo watch -x "run -p onlinerpg-server -- --port 10015"
```

The terrain REST API starts automatically on port 10016.

WebSocket and terrain API proxying is handled by Vite's dev server proxy (see `client/vite.config.ts`), so no separate socat or SSL proxy is needed.


### 4. Running the Client

```bash
cd client
npm install
npm run dev -- --port 10004
```

### 5. Automatic WASM Rebuild on Shared Code Changes (Recommended)
To have Rust code changes in the `shared` library reflected in the browser immediately during client development, run the following command in a separate terminal:

```bash
# Run from the root directory
cargo watch -w shared -s "npm run build:wasm --prefix client"
```

### 6. Running the GLB Editor

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

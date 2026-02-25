# OnlineRPG

An simple online RPG prototype.

## Tech Stack

**Client:**
- Svelte + TypeScript
- Three.js (Threlte)
- Socket.io-client
- Vite

**Server:**
- Rust
- Tokio (async runtime)
- tokio-tungstenite (WebSocket)
- serde (JSON serialization)

## Development Setup

### 1. Prerequisites

- **Rust & Cargo**: [Install Rust](https://rustup.rs/)
- **Node.js & npm**: [Install Node.js](https://nodejs.org/)
- **(Recommended) cargo-watch**: For automatic server restarts on code changes.
  ```bash
  cargo install cargo-watch
  ```

### 2. Running the Server

This project is organized as a **Cargo Workspace**. To detect changes in both the server (`server/`) and shared logic (`shared/`), it is recommended to run commands from the **root directory**.

> **Port Rule:** The client automatically calculates the server address by subtracting 1 from the current browser port. Therefore, if the **server port is N, the client port must be N+1** to connect automatically.

```bash
# Run server (e.g., port 1234)
cargo watch -x "run -p onlinerpg-server -- --port 5172"
```

### 3. Running the Client

#### Default Run (Run on 5173 if server is on 5172)
```bash
cd client
npm install
npm run dev -- --port 5173
```

#### Automatic WASM Rebuild on Shared Code Changes (Recommended)
To have Rust code changes in the `shared` library reflected in the browser immediately during client development, run the following command in a separate terminal:

```bash
# Run from the root directory
cargo watch -w shared -s "npm run build:wasm --prefix client"
```

By default, the client runs on `localhost:5173` and can be accessed via your browser.

## How to Connect

1. Ensure the server is running on `localhost:8080`
2. Run the client on `localhost:5173`
3. Access the game through your browser

## Features

- **Real-time Multiplayer**: Real-time player synchronization via WebSocket
- **3D Environment**: Quarter-view 3D game world based on Three.js
- **Chat System**: Real-time chat functionality
- **Player Movement**: Character control via mouse/keyboard

## Documentation

- Worldbuilding: [WORLD_BUILDING.md](WORLD_BUILDING.md)

## Architecture

- **Client**: Svelte component-based UI + Three.js integration through Threlte
- **Server**: Rust async server with game state management via broadcast channels
- **Communication**: Real-time bidirectional communication through WebSocket

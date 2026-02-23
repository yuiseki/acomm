# acomm

Communication hub for AI agents and human interaction.

`acomm` acts as the nervous system for the `yuiclaw` project, orchestrating multiple AI agent CLIs and providing real-time, multi-channel communication via a bridge architecture.

- **Bridge Architecture**: Decouples UI clients from agent execution via Unix Domain Sockets.
- **Real-time Streaming**: Delivers agent responses in chunks as they are generated.
- **Rich TUI**: Dashboard with CJK support, multiline input, and Emacs-style keybindings.
- **Unified Protocol**: JSON-based event bus for seamless integration of TUI, CLI, and adapters.

## Install

Build and install from source:

```bash
cd /home/yuiseki/Workspaces/repos/acomm
cargo install --path .
```

Run without installing:

```bash
cargo run -q -- --help
```

## Usage

```bash
acomm --help
```

Top-level modes:

- **TUI (Default)**: Interactive dashboard.
- `--bridge`: Background bridge process.
- `--publish <msg>`: Send a message to the bridge.
- `--subscribe`: Monitor the bridge message bus.
- `--dump`: Dump current bridge backlog and exit.
- `--reset`: Reset bridge backlog and session state.

Global options:

- `--channel <name>`: Specify communication channel (default: `tui`).
- `--slack`: Run as a Slack Socket Mode adapter (Milestone 2).

## Quick Start

Start the interactive TUI (automatically starts the bridge if not running):

```bash
acomm
```

Subscribe to the conversation from another terminal:

```bash
acomm --subscribe
```

Publish a message from a script:

```bash
acomm --publish "Scan recent logs for errors" --channel abeat
```

## Main Commands

### `acomm (TUI)`

The primary interface for interaction.

- `i`: Enter **INSERT** mode.
- `Esc`: Back to **NORMAL** mode.
- `q`: Quit.
- `1` - `4`: Switch active AI tool (Gemini, Claude, Codex, OpenCode).
- `PgUp` / `PgDn`: Fast scroll history.

#### INSERT Mode Bindings
- `Ctrl+P` / `Ctrl+N`: Cycle through input history.
- `Ctrl+A` / `Ctrl+E`: Move cursor to beginning/end of line.
- `Ctrl+K`: Kill from cursor to end of line.
- `Ctrl+Y`: Yank last killed text.
- `Shift+Enter`: Insert newline.

### `acomm --bridge`

Starts the centralized messaging hub. Listens on `/tmp/acomm.sock` by default. Manages:
- Agent execution via `acore`.
- Conversation backlog (last 100 events).
- Session logging to `~/.cache/acomm/sessions/`.

### `acomm --publish <msg>`

One-shot message delivery. Supports stdin via `-`:
```bash
echo "Hello" | acomm --publish -
```

### `acomm --subscribe`

Real-time monitoring of all protocol events. Displays a thinking spinner during agent processing.

## Protocol (JSONL)

Communication with the bridge uses JSONL over Unix Domain Sockets.

- `Prompt`: User input.
- `AgentChunk`: Streamed response fragment.
- `AgentDone`: Completion signal.
- `StatusUpdate`: Processing state (thinking).
- `SyncContext`: Memory synchronization.
- `ToolSwitched`: Active tool change.

## Runtime Layout

Default cache root: `~/.cache/acomm`

- `~/.cache/acomm/sessions/`: Daily JSONL session logs.
- `~/.cache/acomm/history.txt`: Persistent TUI input history.
- `/tmp/acomm.sock`: Unix Domain Socket for bridge communication.

## Development

```bash
cargo fmt
cargo test
cargo build
```

# ADR 001: Core Architecture and Tech Stack

## Status
Proposed

## Context
`acomm` (Agentic Communication) aims to bridge AI agents with various communication channels (Discord, Slack, etc.) while providing an experience equivalent to a Linux terminal.
Unlike `amem` (Memory) or `abeat` (Heartbeat), `acomm` must handle real-time, event-driven communication via WebSockets (Discord Gateway, Slack Socket Mode) and maintain long-running connections.

The core requirements are:
- Multi-channel support (Discord, Slack as priority).
- Low-latency interaction (Terminal-like responsiveness).
- Preflight logic (Allowlist, Session routing) to protect the agent loop.
- Metadata pass-through to maintain threading and context.
- Integration with the existing agent ecosystem (`amem`, `abeat`, Gemini CLI).

## Decision
Adopt a Rust-based async architecture with a Trait-based channel abstraction, acting as an orchestrator for existing LLM CLIs.

### 1. Implementation Language and Runtime
- **Language:** Rust.
- **Async Runtime:** `tokio`.
- **TUI Library:** `ratatui` + `crossterm`.
  - Used for the local terminal communication channel.

### 2. CLI-Wrapper Philosophy
- **No Direct API Calls:** `acomm` (and `yuiclaw` as a whole) MUST NOT implement direct LLM API calls (e.g., OpenAI/Gemini REST APIs).
- **Subprocess Orchestration:** Instead, it wraps official/installed CLIs:
  - `gemini-cli`, `claude` (Claude Code), `codex`, `opencode`.
- **Standard Interface:** It maps inbound messages to CLI arguments and parses their stdout/stderr for outbound delivery.

### 3. Channel Abstraction Model
- Use a `Channel` Trait to normalize interactions across different providers.
- **TUI Channel:** A first-class implementation of the `Channel` trait, providing a rich terminal dashboard.
- **External Channels:** Discord, Slack, etc., implemented as background tasks.

### 3. Preflight-First Design
- Implement a dedicated Preflight layer that intercepts inbound events before they reach the agent logic.
- Responsibility:
  - Security (Allowlist, User authorization).
  - Normalization (Converting channel-specific events to generic `SystemEvent`).
  - Session Mapping (Mapping channel/thread IDs to stable agent session keys).

### 4. Terminal Equivalence Features
- Support for **Streaming Responses** (Draft updates) where the channel allows.
- **Typing Indicators** to signal agent activity.
- **Markdown Normalization** to ensure consistency between terminal and chat UI.

### 5. Dependency Selection (Initial)
- `serenity` or `twilight` for Discord.
- `slack-mxc` or custom Socket Mode implementation for Slack.
- `serde` for serialization/deserialization of events.

## Consequences
- Enables a single binary to manage multiple communication channels efficiently.
- Increases initial complexity (async/await) compared to `abeat`, but provides the necessary scalability for real-time communication.
- Provides a clean separation between "how we talk" (Channel) and "what we say" (Agent logic).

# ADR 004: CLI Wrapper Orchestration and TUI Dashboard

## Status
Proposed

## Context
`yuiclaw` aims to be a minimal orchestrator. Instead of re-implementing LLM API clients, it leverages the tools already installed on the user's system (`gemini-cli`, `claude`, etc.). `acomm` must serve as the bridge between these diverse CLI interfaces and the communication channels, including a local TUI for the user.

## Decision
Implement `acomm` as a TUI dashboard that delegates execution to `acore`.

### 1. Delegation to `acore`
- `acomm` does not execute subprocesses directly.
- It captures user input and sends it to `acore` for processing.
- It receives stream events from `acore` and renders them in the TUI or external channels.

### 2. TUI Dashboard (Ratatui)
- The TUI provides a high-fidelity view of the orchestration happening in `acore`.
- **Main Chat:** Renders the conversation stream managed by `acore`.
- **CLI Switcher:** Sends commands to `acore` to switch the active backend.
  - **System Status:** Real-time visibility into `amem` (Memory lookups) and `abeat` (Upcoming heartbeat tasks).
  - **Channel Monitor:** View active inbound/outbound traffic from Discord/Slack.

### 3. "No-Logic" Routing
- `acomm` focuses on **Routing and Formatting**.
- It does not decide "what to say" but "who should say it" based on the user's routing preferences or the current active CLI in the TUI.

### 4. Terminal as a Channel
- The TUI is treated as a high-bandwidth `Channel` implementation.
- While Discord is limited by rate limits and 2000-character blocks, the TUI channel can provide the full, unbridled output of the wrapped CLIs, including rich formatting and immediate feedback.

## Consequences
- **Security:** Leverages the existing security and auth of the wrapped CLIs (no storing of API keys in `acomm` itself).
- **Extensibility:** Adding a new LLM provider is as simple as adding a wrapper for its CLI.
- **Complexity:** Parsing CLI output (which may contain ANSI codes or non-standard formatting) for external channels like Discord will require robust "Normalization" logic.

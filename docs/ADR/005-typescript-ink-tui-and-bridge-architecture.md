# ADR 005: TypeScript/Ink TUI and Unix Socket Bridge Architecture

## Status
Accepted

## Context

ADR 001 and ADR 004 specified `ratatui + crossterm` as the TUI implementation. During development, this approach encountered a fundamental limitation: **full-width CJK character input**.

Japanese kana and kanji each occupy 2 terminal columns, but crossterm's raw input handling tracks cursor position in byte/code-unit terms rather than visual columns. This caused several compounding problems:

- Typing Japanese in the input field caused the cursor to drift from the visible character positions.
- Line wrapping triggered by terminal width was calculated in characters, not visual columns, so Japanese text overflowed the input box and scrolled the screen horizontally.
- Backspace removed incorrect numbers of bytes rather than whole code points in some cases.

Fixing these issues correctly in Rust would have required reimplementing large parts of crossterm's input loop and cursor tracking with visual-width awareness — significant complexity for a component that is not `acomm`'s core value.

At the same time, the Node.js ecosystem has mature, well-tested tooling for exactly this problem:

- [`string-width`](https://github.com/sindresorhus/string-width) accurately measures the terminal column width of any Unicode string, including full-width CJK and emoji.
- [Ink](https://github.com/vadimdemedes/ink) provides a React component model for terminal UIs, making layout composition and re-rendering straightforward.
- The combination is already proven in production by tools like `gemini-cli`.

Additionally, separating the TUI from the bridge server creates a cleaner architecture: the Rust binary owns the stateful, long-running socket server, and the TypeScript process owns the ephemeral, user-facing display.

## Decision

Split the TUI and the bridge into two cooperating processes connected by a Unix domain socket.

### 1. `acomm --bridge` — Rust Unix socket server

- Binds `/tmp/acomm.sock`.
- Accepts multiple clients simultaneously (TUI, `--publish`, external scripts).
- Maintains the agent session and streams `AgentChunk` / `AgentDone` events to all connected clients.
- Replays the full event backlog to newly connected clients so sessions can be resumed.
- Remains a pure server; it has no terminal UI of its own.

### 2. `acomm-tui` — TypeScript/Ink TUI client

- Located in `tui/` within the `acomm` repository.
- Connects to the bridge socket on startup; if no socket exists it spawns `acomm --bridge` automatically.
- Renders the conversation in a scrollable message area and a multiline input box.
- Implements CJK-aware line wrapping using `string-width` and a custom `wrapLine()` helper.
- Each visual row is rendered as `<Box height={1}>` to prevent Ink's layout engine from miscounting double-width characters.
- Persists input history to `~/.cache/acomm/history.txt`.
- Is the **preferred** TUI entry point when available in `PATH`.

### 3. Protocol

Events are newline-delimited JSON over the Unix socket.

Key event types:

| Event | Direction | Description |
|---|---|---|
| `Prompt` | client → bridge → clients | User input; echoed back for display |
| `AgentChunk` | bridge → clients | Streaming response fragment |
| `AgentDone` | bridge → clients | Response complete |
| `StatusUpdate` | bridge → clients | Processing flag for spinner |
| `ToolSwitched` | bridge → clients | Active LLM tool changed |
| `SyncContext` | bridge → clients | `amem today` snapshot injected |
| `SystemMessage` | bridge → clients | Bridge-level status notice |

### 4. Rust TUI as fallback

The original Rust TUI (`acomm` without `--bridge`) remains in place as a functional fallback for environments where Node.js is unavailable. `yuiclaw` prefers `acomm-tui` (TypeScript) and falls back to `acomm` (Rust) if the former is not in `PATH`.

### 5. CJK input implementation

`acomm-tui` implements CJK-aware input through the following helpers in `textHelpers.ts`:

- `visualWidth(str)` — wraps `string-width` for terminal column measurement.
- `wrapLine(line, maxWidth)` — splits a logical line into `VisualChunk[]` segments that each fit within `maxWidth` terminal columns, advancing the break point by visual width rather than character count.
- `deleteCharBefore(text, offset)` — uses `Array.from()` to split by Unicode code points, ensuring backspace removes exactly one grapheme cluster regardless of its UTF-16 length.
- `offsetToRowCol` / `rowColToOffset` — convert between flat UTF-16 offsets and (row, col) coordinates across multi-line input.

## Consequences

- **Better UX for CJK users**: Japanese and Chinese input no longer causes horizontal overflow or cursor drift.
- **Cleaner separation of concerns**: The Rust binary is a headless bridge server; the TypeScript process is a stateless display client. Either can be restarted independently.
- **Multi-client capability**: Because the bridge is a persistent socket server, external scripts can inject messages via `acomm --publish` (or `yuiclaw pub`) while the TUI is running, enabling the `abeat` heartbeat integration.
- **Session resumption**: Backlog replay on reconnect means the full conversation history is visible when the TUI is restarted mid-session.
- **Added runtime dependency**: The preferred TUI requires Node.js. The Rust fallback avoids this dependency but lacks full CJK support.
- **Supersedes**: The Ratatui TUI implementation described in ADR 001 (§ Implementation Language, TUI Library) and ADR 004 (§ TUI Dashboard).

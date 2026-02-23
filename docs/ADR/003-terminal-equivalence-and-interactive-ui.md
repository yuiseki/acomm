# ADR 003: Terminal Equivalence and Interactive UI

## Status
Proposed

## Context
The primary goal of `acomm` is to provide an experience equivalent to interacting with an agent in a Linux terminal. In a terminal, users see real-time output, progress indicators, and have immediate access to commands. Traditional bots often feel static or "black-boxed" by comparison.

## Decision
Design the communication flow to prioritize real-time feedback and normalize interactive elements into the standard message pipeline.

### 1. Visual Feedback (Typing and Reactions)
- **Typing Indicators:** Channel adapters MUST trigger "typing" status as soon as the agent begins processing. For long-running tasks, this status should be periodically refreshed.
- **Progress Reactions:** Use transient reactions (e.g., Slack's `:eyes:`) to indicate specific stages of processing (e.g., "Thinking", "Searching Memory", "Executing Tool").

### 2. Streaming and Draft Updates
- Where the channel API supports it (e.g., Slack message updates, Discord message edits), `acomm` will implement **Draft Streaming**.
- The agent's partial output is sent as a "draft" message and updated incrementally, mimicking the stdout behavior of a terminal.

### 3. Normalization of Interactive Surfaces
- **Slash Commands:** Map native slash commands (e.g., `/memory search`, `/heartbeat check`) to the same internal command logic used by the terminal CLI.
- **Buttons and Modals:** Normalize button clicks or modal submissions into `SystemEvents`. For example, a "Retry" button click should be treated as a re-submission of the previous prompt with a "retry" flag.

### 4. Terminal-Like Formatting
- **Markdown Preservation:** Strictly maintain Markdown formatting (code blocks, bold, lists).
- **ANSI to UI Mapping:** If the agent output contains ANSI escape codes (for colors/styles), the adapter should attempt to map these to channel-native formatting (e.g., Discord's syntax-highlighted code blocks).

### 5. Activity Logging (Integration with `amem`)
- Every interaction via `acomm` should be logged to `.amem/agent/activity/` just like terminal sessions.
- This ensures that the agent "remembers" what was discussed on Discord/Slack when the user returns to the terminal.

## Consequences
- High-quality, "alive" feeling interactions that distinguish `acomm` from simple request-response bots.
- Consistent behavior across CLI and Chat platforms, reducing user cognitive load.
- Increased API call volume due to frequent updates (Streaming/Typing), requiring careful rate-limit management.

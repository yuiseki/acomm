# ADR 002: Preflight Routing and Metadata Strategy

## Status
Proposed

## Context
Communication channels have diverse ways of identifying users, channels, and threads (e.g., Discord's Snowflakes, Slack's `thread_ts`). AI agents need a stable "Session ID" to retrieve memory and maintain context, but they also need to return responses to the correct thread or even update previous messages.

## Decision
Implement a Preflight Routing layer that maps channel-specific identifiers to stable session keys while preserving original context as transparent Metadata.

### 1. Session Mapping Logic
- Every inbound message is processed to resolve a `SessionKey`.
- Default mapping:
  - **Discord:** `discord:{guild_id}:{channel_id}[:{thread_id}]`
  - **Slack:** `slack:{team_id}:{channel_id}[:{thread_ts}]`
- This mapping ensures that `amem` can retrieve the correct context based on the communication boundary.

### 2. Peer and User Identification
- Separate `UserKey` (e.g., `discord_user:{user_id}`) from `SessionKey`.
- Implement an **Allowlist** at the Preflight stage. Messages from unauthorized users or channels are dropped before reaching the LLM/Agent loop to save costs and ensure security.

### 3. Metadata Pass-through
- Inbound messages are wrapped in an `Envelope` containing:
  - `payload`: The actual text or command.
  - `metadata`: A key-value map of channel-specific data (e.g., `message_id`, `reply_to`, `is_edit`).
- When the Agent generates an `OutboundMessage`, the original `metadata` is passed back to the Channel Adapter to ensure the reply is routed correctly (e.g., as a thread reply or an update to a previous draft).

### 4. Pairing Mechanism (Optional for DM)
- For Direct Messages, implement a "Pairing" flow:
  - An unknown user sends a message.
  - `acomm` sends a notification to the Owner (via a configured "Admin Channel").
  - The Owner approves/denies the pairing.
  - This prevents the agent from being abused by random users.

## Consequences
- The Agent logic remains "Channel-Agnostic" while still being "Context-Aware".
- Metadata-driven replies allow for advanced UX features like editing a message while the AI is "thinking" or "streaming".
- Centralized allowlist simplifies security management across multiple platforms.

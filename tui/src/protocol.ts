/**
 * Protocol type definitions for the acomm bridge.
 * Mirrors Rust's ProtocolEvent with serde external tagging.
 *
 * Example JSON from the bridge:
 *   {"Prompt":{"text":"hello","tool":null,"channel":"tui"}}
 *   {"AgentChunk":{"chunk":"...","channel":null}}
 *   {"AgentDone":{"channel":null}}
 *   {"ToolSwitched":{"tool":"Gemini"}}
 */

export type AgentTool = 'Gemini' | 'Claude' | 'Codex' | 'OpenCode' | 'Mock';

export const AGENT_TOOLS: AgentTool[] = ['Gemini', 'Claude', 'Codex', 'OpenCode'];

export type ProtocolEvent =
  | { Prompt: { text: string; tool: AgentTool | null; channel: string | null } }
  | { AgentChunk: { chunk: string; channel: string | null } }
  | { AgentDone: { channel: string | null } }
  | { SystemMessage: { msg: string; channel: string | null } }
  | { StatusUpdate: { is_processing: boolean; channel: string | null } }
  | { SyncContext: { context: string } }
  | { ToolSwitched: { tool: AgentTool } };

/** Returns the variant name of a ProtocolEvent. */
export function eventKind(event: ProtocolEvent): string {
  return Object.keys(event)[0]!;
}

/** Converts an AgentTool to its CLI command name (lowercase). */
export function toolCommandName(tool: AgentTool): string {
  return tool.toLowerCase();
}

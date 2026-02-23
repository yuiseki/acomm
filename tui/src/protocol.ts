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

/** Available models for each provider. */
export const PROVIDER_MODELS: Record<AgentTool, string[]> = {
  Gemini:   ['gemini-2.5-flash', 'gemini-2.5-pro', 'gemini-2.0-flash'],
  Claude:   ['claude-opus-4-6', 'claude-sonnet-4-6', 'claude-haiku-4-5'],
  Codex:    ['gpt-4o', 'gpt-4o-mini', 'o1-mini'],
  OpenCode: ['default'],
  Mock:     ['mock-model'],
};

export type ProtocolEvent =
  | { Prompt: { text: string; tool: AgentTool | null; channel: string | null } }
  | { AgentChunk: { chunk: string; channel: string | null } }
  | { AgentDone: { channel: string | null } }
  | { SystemMessage: { msg: string; channel: string | null } }
  | { StatusUpdate: { is_processing: boolean; channel: string | null } }
  | { SyncContext: { context: string } }
  | { ToolSwitched: { tool: AgentTool } }
  | { ModelSwitched: { model: string } };

/** Returns the variant name of a ProtocolEvent. */
export function eventKind(event: ProtocolEvent): string {
  return Object.keys(event)[0]!;
}

/** Converts an AgentTool to its CLI command name (lowercase). */
export function toolCommandName(tool: AgentTool): string {
  return tool.toLowerCase();
}

/** Case-insensitively finds the valid AgentTool for a given string. */
export function normalizeTool(name: string | null | undefined): AgentTool {
  if (!name) return 'Gemini';
  const target = name.toLowerCase();
  for (const tool of AGENT_TOOLS) {
    if (tool.toLowerCase() === target) return tool;
  }
  if ('mock'.toLowerCase() === target) return 'Mock';
  return 'Gemini';
}

/** Safely gets the model list for a tool, falling back to Gemini if the tool is invalid. */
export function getModelsForTool(tool: AgentTool | string | null | undefined): string[] {
  const normalized = normalizeTool(tool as any);
  return PROVIDER_MODELS[normalized] ?? PROVIDER_MODELS['Gemini'];
}

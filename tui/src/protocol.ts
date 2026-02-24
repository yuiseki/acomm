/**
 * Protocol type definitions for the acomm bridge.
 * Mirrors Rust's ProtocolEvent with serde external tagging.
 *
 * Example JSON from the bridge:
 *   {"Prompt":{"text":"hello","provider":null,"channel":"tui"}}
 *   {"AgentChunk":{"chunk":"...","channel":null}}
 *   {"AgentDone":{"channel":null}}
 *   {"ProviderSwitched":{"provider":"Gemini"}}
 */

export type AgentProvider = 'Gemini' | 'Claude' | 'Codex' | 'OpenCode' | 'Mock';

export const AGENT_PROVIDERS: AgentProvider[] = ['Gemini', 'Claude', 'Codex', 'OpenCode'];

/** Available models for each provider. */
export const PROVIDER_MODELS: Record<AgentProvider, string[]> = {
  Gemini:   ['gemini-2.5-flash', 'gemini-2.5-pro', 'gemini-2.0-flash'],
  Claude:   ['claude-opus-4-6', 'claude-sonnet-4-6', 'claude-haiku-4-5'],
  Codex:    ['gpt-4o', 'gpt-4o-mini', 'o1-mini'],
  OpenCode: ['default'],
  Mock:     ['mock-model'],
};

export type ProtocolEvent =
  | { Prompt: { text: string; provider: AgentProvider | null; channel: string | null } }
  | { AgentChunk: { chunk: string; channel: string | null } }
  | { AgentDone: { channel: string | null } }
  | { SystemMessage: { msg: string; channel: string | null } }
  | { StatusUpdate: { is_processing: boolean; channel: string | null } }
  | { BridgeSyncDone: {} }
  | { SyncContext: { context: string } }
  | { ProviderSwitched: { provider: AgentProvider } }
  | { ModelSwitched: { model: string } };

/** Returns the variant name of a ProtocolEvent. */
export function eventKind(event: ProtocolEvent): string {
  return Object.keys(event)[0]!;
}

/** Converts an AgentProvider to its CLI command name (lowercase). */
export function providerCommandName(provider: AgentProvider): string {
  return provider.toLowerCase();
}

/** Case-insensitively finds the valid AgentProvider for a given string. */
export function normalizeProvider(name: string | null | undefined): AgentProvider {
  if (!name) return 'Gemini';
  const target = name.toLowerCase();
  for (const provider of AGENT_PROVIDERS) {
    if (provider.toLowerCase() === target) return provider;
  }
  if ('mock'.toLowerCase() === target) return 'Mock';
  return 'Gemini';
}

/** Safely gets the model list for a provider, falling back to Gemini if the provider is invalid. */
export function getModelsForProvider(provider: AgentProvider | string | null | undefined): string[] {
  const normalized = normalizeProvider(provider as any);
  return PROVIDER_MODELS[normalized] ?? PROVIDER_MODELS['Gemini'];
}

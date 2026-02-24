import type { AgentProvider } from './protocol.js';
import { normalizeProvider, providerCommandName } from './protocol.js';

/**
 * Returns a slash command to reconcile the bridge provider with the startup provider.
 * Returns null when no reconciliation is needed.
 */
export function buildInitialProviderSyncCommand(
  desiredProvider: AgentProvider | string | null | undefined,
  currentBridgeProvider: AgentProvider | string | null | undefined,
): string | null {
  const desired = normalizeProvider(desiredProvider as any);
  const current = normalizeProvider(currentBridgeProvider as any);
  if (desired === current) return null;
  return `/provider ${providerCommandName(desired)}`;
}

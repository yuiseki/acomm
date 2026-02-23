/**
 * Pure helper for slash command parsing in the TUI input field.
 *
 * Commands handled locally (not forwarded to the bridge):
 *   /provider  — open provider selection menu
 *   /model     — open model selection menu for current provider
 *   /clear     — clear local messages and bridge session
 *   /reset     — alias for /clear
 *
 * Everything else is forwarded to the bridge as-is.
 */

export type SlashAction =
  | { type: 'provider' }
  | { type: 'model' }
  | { type: 'clear' }
  | { type: 'bridge-forward'; text: string };

/**
 * Parse a slash command string into a SlashAction.
 * Returns null if the input does not start with '/'.
 */
export function parseSlashCommand(text: string): SlashAction | null {
  const trimmed = text.trim();
  if (!trimmed.startsWith('/')) return null;

  const cmd = trimmed.slice(1).split(/\s+/)[0]?.toLowerCase() ?? '';

  switch (cmd) {
    case 'provider': return { type: 'provider' };
    case 'model':    return { type: 'model' };
    case 'clear':
    case 'reset':    return { type: 'clear' };
    default:         return { type: 'bridge-forward', text: trimmed };
  }
}

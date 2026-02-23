/**
 * Pure helper for slash command parsing and autocomplete in the TUI input field.
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

// ---------- Autocomplete definitions ----------

export interface SlashCommandDef {
  /** Command name without the leading slash (e.g. "provider"). */
  command: string;
  /** Short description shown in the autocomplete dropdown. */
  description: string;
}

/**
 * All known slash commands, in display order.
 * Shown in the autocomplete dropdown when the user types '/'.
 */
export const SLASH_COMMANDS: SlashCommandDef[] = [
  { command: 'provider', description: 'Open provider selection menu' },
  { command: 'model',    description: 'Open model selection menu for current provider' },
  { command: 'clear',    description: 'Clear messages and reset bridge session' },
  { command: 'reset',    description: 'Alias for /clear' },
];

/**
 * Return the slash commands whose name starts with the current input prefix.
 *
 * @param input  The raw input value (e.g. "/p", "/mo", "/").
 * @returns      Matching SlashCommandDef entries, or [] when input doesn't
 *               start with '/' or the command word already contains a space
 *               (meaning the user is past the command-name phase).
 */
export function getSlashCompletions(input: string): SlashCommandDef[] {
  if (!input.startsWith('/')) return [];
  const prefix = input.slice(1).toLowerCase();
  // Stop suggesting once there's a space — the command name is already chosen.
  if (prefix.includes(' ')) return [];
  return SLASH_COMMANDS.filter((cmd) => cmd.command.startsWith(prefix));
}

// ---------- Command parser ----------

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

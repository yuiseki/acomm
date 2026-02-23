/**
 * Persistent session turn storage for the acomm TUI.
 *
 * Each completed conversation turn (user prompt + agent response) is appended
 * as a JSON line to a dated file:
 *
 *   ~/.cache/acomm/sessions/YYYY-MM-DD.jsonl
 *
 * The `loadRecentTurns()` function reads the N most recent turns across all
 * session files for display in the session browser.
 */

import {
  appendFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
} from 'node:fs';
import { join } from 'node:path';
import { homedir } from 'node:os';

// ---------- types ----------

export interface SessionTurn {
  timestamp: string; // ISO 8601 — when AgentDone fired
  tool: string;      // AgentTool variant name, e.g. "Gemini"
  model: string;     // Active model name, e.g. "gemini-2.5-flash"
  prompt: string;    // User's prompt text
  response: string;  // Full agent response (accumulated chunks)
}

// ---------- default path ----------

export const DEFAULT_SESSIONS_DIR = join(homedir(), '.cache', 'acomm', 'sessions');

// ---------- helpers ----------

/**
 * Ensure the sessions directory exists (idempotent).
 */
export function makeSessionsDir(dir: string = DEFAULT_SESSIONS_DIR): void {
  mkdirSync(dir, { recursive: true });
}

/**
 * Extract the date string YYYY-MM-DD from an ISO 8601 timestamp.
 */
function dateFromTimestamp(ts: string): string {
  return ts.slice(0, 10); // "2026-02-24T10:00:00Z" → "2026-02-24"
}

// ---------- public API ----------

/**
 * Append one session turn to the dated JSONL file.
 * Creates the sessions directory and file if they do not exist.
 *
 * @param turn        The conversation turn to save.
 * @param sessionsDir Override the storage directory (useful for tests).
 */
export function saveSessionTurn(
  turn: SessionTurn,
  sessionsDir: string = DEFAULT_SESSIONS_DIR,
): void {
  try {
    makeSessionsDir(sessionsDir);
    const date = dateFromTimestamp(turn.timestamp);
    const file = join(sessionsDir, `${date}.jsonl`);
    appendFileSync(file, JSON.stringify(turn) + '\n', 'utf8');
  } catch {
    // Non-fatal — session saving must never crash the TUI.
  }
}

/**
 * Load the most recent session turns, newest-first within each file,
 * files sorted by filename (i.e. date, oldest first → recent lines last).
 * Returns at most `limit` turns, all fields populated.
 *
 * @param limit       Maximum number of turns to return.
 * @param sessionsDir Override the storage directory (useful for tests).
 */
export function loadRecentTurns(
  limit: number,
  sessionsDir: string = DEFAULT_SESSIONS_DIR,
): SessionTurn[] {
  if (!existsSync(sessionsDir)) return [];

  // Collect all .jsonl files sorted by filename (dates sort lexicographically).
  const files = readdirSync(sessionsDir)
    .filter((f) => f.endsWith('.jsonl'))
    .sort();

  if (files.length === 0) return [];

  const turns: SessionTurn[] = [];

  for (const file of files) {
    try {
      const content = readFileSync(join(sessionsDir, file), 'utf8');
      for (const line of content.split('\n')) {
        const trimmed = line.trim();
        if (!trimmed) continue;
        try {
          const parsed = JSON.parse(trimmed) as SessionTurn;
          if (parsed.timestamp && parsed.tool && parsed.prompt !== undefined) {
            turns.push(parsed);
          }
        } catch {
          // Skip malformed lines.
        }
      }
    } catch {
      // Skip unreadable files.
    }
  }

  // Return the last `limit` entries (most recent).
  return turns.slice(-limit);
}

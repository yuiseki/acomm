/**
 * Session history browser component.
 *
 * Shows a paginated list of past conversation turns loaded from
 * ~/.cache/acomm/sessions/*.jsonl.
 *
 * Key bindings:
 *   ↑ / ↓  — navigate the list
 *   Esc     — close the browser and return to input
 */

import React from 'react';
import { Box, Text } from 'ink';
import chalk from 'chalk';
import type { SessionTurn } from './sessionStorage.js';

interface Props {
  turns: SessionTurn[];
  selectedIndex: number;
}

/** Format an ISO timestamp to a compact local-time string: "02-24 10:30" */
function formatTimestamp(ts: string): string {
  try {
    const d = new Date(ts);
    const mm = String(d.getMonth() + 1).padStart(2, '0');
    const dd = String(d.getDate()).padStart(2, '0');
    const hh = String(d.getHours()).padStart(2, '0');
    const mi = String(d.getMinutes()).padStart(2, '0');
    return `${mm}-${dd} ${hh}:${mi}`;
  } catch {
    return ts.slice(0, 16);
  }
}

/** Truncate a string to maxLen, appending '…' if needed. */
function truncate(str: string, maxLen: number): string {
  if (str.length <= maxLen) return str;
  return str.slice(0, maxLen - 1) + '…';
}

export default function SessionBrowser({ turns, selectedIndex }: Props): React.JSX.Element {
  const termWidth = process.stdout.columns ?? 80;
  // Reserve space for timestamp(12) + tool(10) + separators(4) + prompt preview
  const promptWidth = Math.max(20, termWidth - 34);

  return (
    <Box flexDirection="column" borderStyle="single" borderColor="cyan">
      <Box paddingLeft={1}>
        <Text>{chalk.cyan('Session History')}{chalk.dim('  ↑/↓=navigate  Esc=close')}</Text>
      </Box>
      {turns.length === 0 ? (
        <Box paddingLeft={1}>
          <Text>{chalk.dim('No session history yet. Start a conversation to begin logging.')}</Text>
        </Box>
      ) : (
        turns.map((turn, i) => {
          const isSelected = i === selectedIndex;
          const time = formatTimestamp(turn.timestamp);
          const tool = truncate(`${turn.tool}`, 9).padEnd(9);
          const prompt = truncate(turn.prompt.replace(/\n/g, ' '), promptWidth);

          const line = `${time}  ${tool}  ${prompt}`;

          return (
            <Box key={`${turn.timestamp}-${i}`} paddingLeft={1}>
              <Text>
                {isSelected
                  ? chalk.bgCyan(chalk.black(line))
                  : chalk.dim(time) + '  ' + chalk.green(tool) + '  ' + prompt}
              </Text>
            </Box>
          );
        })
      )}
    </Box>
  );
}

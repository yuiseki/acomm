/**
 * Virtualized message list for the acomm TUI.
 *
 * Renders only the visible window of lines from the full message history,
 * allowing efficient display of arbitrarily long conversations.
 *
 * Flattening strategy:
 *   - Each Message is converted to a display string (prefix + rendered body)
 *   - The string is split by '\n' into individual terminal lines
 *   - Only the slice [scrollLine, scrollLine + visibleHeight] is rendered
 *
 * The caller (App.tsx) owns scrollLine and autoScroll state; this component
 * is purely presentational.
 */

import React from 'react';
import { Box, Text } from 'ink';
import chalk from 'chalk';
import { renderMarkdown } from './renderMarkdown.js';
import { wrapLine } from './textHelpers.js';

const SPINNER = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

// ---------- Message shape (mirrored from App.tsx) ----------

export interface VMessage {
  id: number;
  prefix: string;
  text: string;
  isAgent: boolean;
  isStreaming: boolean;
}

// ---------- flatten helpers ----------

/**
 * Convert a single Message to its ANSI display string (prefix + body).
 * Applies markdown rendering for complete agent messages.
 */
function messageToString(
  m: VMessage,
  isLast: boolean,
  awaitingFirstChunk: boolean,
  spinnerIdx: number,
): string {
  const body = m.isAgent && !m.isStreaming ? renderMarkdown(m.text) : m.text;
  if (isLast && awaitingFirstChunk) {
    return m.prefix + body + chalk.yellow(`${SPINNER[spinnerIdx]} thinking...`);
  }
  return m.prefix + body;
}

/**
 * Flatten all messages into individual terminal lines.
 * Returns an array of strings, one per visual line.
 */
export function flattenMessages(
  messages: VMessage[],
  awaitingFirstChunk: boolean,
  spinnerIdx: number,
  maxWidth?: number,
): string[] {
  const lines: string[] = [];
  for (let i = 0; i < messages.length; i++) {
    const m = messages[i]!;
    const str = messageToString(m, i === messages.length - 1, awaitingFirstChunk, spinnerIdx);
    for (const logicalLine of str.split('\n')) {
      if (maxWidth && maxWidth > 0) {
        for (const chunk of wrapLine(logicalLine, maxWidth)) {
          lines.push(chunk.text);
        }
      } else {
        lines.push(logicalLine);
      }
    }
  }
  return lines;
}

// ---------- component ----------

interface Props {
  messages: VMessage[];
  scrollLine: number;
  visibleHeight: number;
  awaitingFirstChunk: boolean;
  spinnerIdx: number;
}

export default function VirtualizedMessageList({
  messages,
  scrollLine,
  visibleHeight,
  awaitingFirstChunk,
  spinnerIdx,
}: Props): React.JSX.Element {
  const termWidth = Math.max(1, process.stdout.columns ?? 80);
  const allLines = flattenMessages(messages, awaitingFirstChunk, spinnerIdx, termWidth);
  const totalLines = allLines.length;
  const start = Math.max(0, Math.min(scrollLine, totalLines - visibleHeight));
  const visibleLines = allLines.slice(start, start + visibleHeight);

  // Scroll position indicator (shown only when scrolled up)
  const isAtBottom = start + visibleHeight >= totalLines;
  const linesAbove = start;

  return (
    <Box flexDirection="column" flexGrow={1} overflow="hidden">
      {/* "Scrolled up" indicator */}
      {linesAbove > 0 && (
        <Box>
          <Text>{chalk.dim(`↑ ${linesAbove} line${linesAbove === 1 ? '' : 's'} above  PgUp/PgDn to scroll`)}</Text>
        </Box>
      )}
      {/* Visible message lines */}
      <Box flexDirection="column" flexGrow={1} overflow="hidden">
        {visibleLines.map((line, i) => (
          <Box key={start + i} height={1} overflow="hidden">
            <Text>{line || ' '}</Text>
          </Box>
        ))}
      </Box>
      {/* "More below" indicator */}
      {!isAtBottom && (
        <Box>
          <Text>{chalk.dim(`↓ ${totalLines - (start + visibleHeight)} more line${totalLines - (start + visibleHeight) === 1 ? '' : 's'} below`)}</Text>
        </Box>
      )}
    </Box>
  );
}

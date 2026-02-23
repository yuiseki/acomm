/**
 * Multiline text input component for Ink.
 *
 * Key bindings:
 *   Enter (no shift)    — submit
 *   Shift+Enter         — insert newline  (requires @jrichman/ink which exposes key.shift)
 *   Alt+Enter / Ctrl+J  — insert newline  (fallback for terminals without Kitty protocol)
 *   Backspace           — delete character before cursor
 *   Left / Right        — move cursor horizontally (by one code point)
 *   Up / Down           — move cursor to previous/next logical line
 *   Ctrl+A / Home       — move to start of line
 *   Ctrl+E / End        — move to end of line
 *   Ctrl+P              — history up
 *   Ctrl+N              — history down
 *
 * Japanese / CJK support:
 *   Each full-width character occupies 2 terminal columns. This component
 *   manually wraps input text at the available column width using string-width
 *   so that Japanese input never causes horizontal overflow.
 */

import React, { useCallback } from 'react';
import { Box, Text, useInput } from 'ink';
import chalk from 'chalk';
import {
  insertAt,
  deleteCharBefore,
  offsetToRowCol,
  rowColToOffset,
  wrapLine,
} from './textHelpers.js';

interface Props {
  value: string;
  cursorOffset: number; // UTF-16 code-unit offset into value
  isProcessing: boolean;
  activeTool: string;
  isActive?: boolean; // when false, input handling is suspended (e.g. during menu mode)
  onChangeCursor: (offset: number) => void;
  onChangeValue: (value: string, cursor: number) => void;
  onSubmit: (value: string) => void;
  onHistoryUp: () => void;
  onHistoryDown: () => void;
}

export default function MultilineInput({
  value,
  cursorOffset,
  isProcessing,
  activeTool,
  isActive = true,
  onChangeCursor,
  onChangeValue,
  onSubmit,
  onHistoryUp,
  onHistoryDown,
}: Props): React.JSX.Element {
  const handleInput = useCallback(
    (input: string, key: ReturnType<typeof useInput extends (h: (i: string, k: infer K) => void) => void ? never : never> extends never ? any : any) => {
      if (isProcessing) return;

      // --- newline: Shift+Enter, Alt+Enter, Ctrl+J ---
      const isNewline =
        (key.return && key.shift) ||
        (key.return && key.meta) ||
        (key.return && key.alt) ||
        (input === '\n' && !key.return); // Ctrl+J sends \n directly

      if (isNewline) {
        const next = insertAt(value, cursorOffset, '\n');
        onChangeValue(next, cursorOffset + 1);
        return;
      }

      // --- submit: plain Enter ---
      if (key.return) {
        if (value.trim()) onSubmit(value);
        return;
      }

      // --- backspace ---
      if (key.backspace || key.delete) {
        const result = deleteCharBefore(value, cursorOffset);
        onChangeValue(result.text, result.cursor);
        return;
      }

      // --- cursor movement ---
      if (key.leftArrow) {
        if (cursorOffset > 0) {
          // Move back one code point
          const before = value.slice(0, cursorOffset);
          const cps = Array.from(before);
          const newBefore = cps.slice(0, -1).join('');
          onChangeCursor(newBefore.length);
        }
        return;
      }
      if (key.rightArrow) {
        if (cursorOffset < value.length) {
          // Move forward one code point
          const rest = value.slice(cursorOffset);
          const nextCp = Array.from(rest)[0] ?? '';
          onChangeCursor(cursorOffset + nextCp.length);
        }
        return;
      }
      if (key.upArrow) {
        const [row, col] = offsetToRowCol(value, cursorOffset);
        if (row > 0) onChangeCursor(rowColToOffset(value, row - 1, col));
        return;
      }
      if (key.downArrow) {
        const [row, col] = offsetToRowCol(value, cursorOffset);
        const lines = value.split('\n');
        if (row < lines.length - 1) onChangeCursor(rowColToOffset(value, row + 1, col));
        return;
      }

      // Ctrl+A / Home — start of current line
      if ((key.ctrl && input === 'a') || key.home) {
        const [row] = offsetToRowCol(value, cursorOffset);
        onChangeCursor(rowColToOffset(value, row, 0));
        return;
      }
      // Ctrl+E / End — end of current line
      if ((key.ctrl && input === 'e') || key.end) {
        const [row] = offsetToRowCol(value, cursorOffset);
        const lineLen = value.split('\n')[row]?.length ?? 0;
        onChangeCursor(rowColToOffset(value, row, lineLen));
        return;
      }

      // Ctrl+P — history up
      if (key.ctrl && input === 'p') {
        onHistoryUp();
        return;
      }
      // Ctrl+N — history down
      if (key.ctrl && input === 'n') {
        onHistoryDown();
        return;
      }

      // Escape — ignore
      if (key.escape) return;

      // --- regular character input ---
      if (!key.ctrl && !key.meta && input) {
        const next = insertAt(value, cursorOffset, input);
        onChangeValue(next, cursorOffset + input.length);
      }
    },
    [value, cursorOffset, isProcessing, onChangeCursor, onChangeValue, onSubmit, onHistoryUp, onHistoryDown],
  );

  useInput(handleInput as any, { isActive });

  // ---------------------------------------------------------------------------
  // Visual rendering with full-width (CJK) aware line wrapping
  // ---------------------------------------------------------------------------

  // Available text width: terminal columns minus border(2) + paddingLeft(1) + safety(1)
  const inputWidth = Math.max(20, (process.stdout.columns ?? 80) - 4);

  const lines = value.split('\n');
  const [cursorRow, cursorCol] = offsetToRowCol(value, cursorOffset);

  // Convert UTF-16 col to code-point index within the cursor's logical line
  const cursorLine = lines[cursorRow] ?? '';
  const cursorCpIdx = Array.from(cursorLine.slice(0, cursorCol)).length;

  // Build visual rows: each logical line is wrapped into VisualChunks
  const visualRows: React.JSX.Element[] = [];

  lines.forEach((line, row) => {
    const chunks = wrapLine(line, inputWidth);
    const isCurrentRow = row === cursorRow;

    chunks.forEach((chunk, ci) => {
      const key = `${row}-${ci}`;
      const isLastChunk = ci === chunks.length - 1;

      if (!isCurrentRow) {
        visualRows.push(
          <Box key={key} height={1}>
            <Text>{chunk.text || ' '}</Text>
          </Box>,
        );
        return;
      }

      // Determine if cursor falls in this chunk
      const cursorInChunk =
        cursorCpIdx >= chunk.startCpIdx && cursorCpIdx < chunk.endCpIdx;
      const cursorAtEnd = isLastChunk && cursorCpIdx >= chunk.endCpIdx;

      if (cursorInChunk || cursorAtEnd) {
        const relIdx = cursorCpIdx - chunk.startCpIdx;
        const cps = Array.from(chunk.text);
        const before = cps.slice(0, relIdx).join('');
        const cursorChar = cps[relIdx] ?? ' ';
        const after = cps.slice(relIdx + 1).join('');
        visualRows.push(
          <Box key={key} height={1}>
            <Text>
              {before}
              {chalk.inverse(cursorChar)}
              {after}
            </Text>
          </Box>,
        );
      } else {
        visualRows.push(
          <Box key={key} height={1}>
            <Text>{chunk.text}</Text>
          </Box>,
        );
      }
    });
  });

  const hint = isProcessing
    ? chalk.dim('  thinking...')
    : chalk.dim(`  [${activeTool}]  Enter=send  Shift+Enter=newline  Ctrl+P/N=history`);

  return (
    <Box flexDirection="column" borderStyle="single" borderColor={isProcessing ? 'yellow' : 'cyan'}>
      <Box flexDirection="column" paddingLeft={1}>
        {visualRows}
      </Box>
      <Text>{hint}</Text>
    </Box>
  );
}

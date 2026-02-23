/**
 * Multiline text input component for Ink.
 *
 * Key bindings:
 *   Enter (no shift)    — submit
 *   Shift+Enter         — insert newline  (requires @jrichman/ink which exposes key.shift)
 *   Alt+Enter / Ctrl+J  — insert newline  (fallback for terminals without Kitty protocol)
 *   Backspace           — delete character before cursor
 *   Left / Right        — move cursor horizontally
 *   Up / Down           — move cursor vertically across lines
 *   Ctrl+A / Home       — move to start of line
 *   Ctrl+E / End        — move to end of line
 *   Ctrl+P              — history up
 *   Ctrl+N              — history down
 */

import React, { useCallback } from 'react';
import { Box, Text, useInput } from 'ink';
import chalk from 'chalk';
import { insertAt, deleteCharBefore, offsetToRowCol, rowColToOffset } from './textHelpers.js';

interface Props {
  value: string;
  cursorOffset: number; // byte offset into value
  isProcessing: boolean;
  activeTool: string;
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
        onChangeCursor(Math.max(0, cursorOffset - 1));
        return;
      }
      if (key.rightArrow) {
        onChangeCursor(Math.min(value.length, cursorOffset + 1));
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

  useInput(handleInput as any);

  // Render each line; highlight the character at the cursor position
  const lines = value.split('\n');
  const [cursorRow, cursorCol] = offsetToRowCol(value, cursorOffset);

  const hint = isProcessing
    ? chalk.dim('  thinking...')
    : chalk.dim(`  [${activeTool}]  Enter=send  Shift+Enter=newline  Ctrl+P/N=history`);

  return (
    <Box flexDirection="column" borderStyle="single" borderColor={isProcessing ? 'yellow' : 'cyan'}>
      <Box flexDirection="column" paddingLeft={1}>
        {lines.map((line, row) => {
          if (row !== cursorRow) {
            return <Text key={row}>{line || ' '}</Text>;
          }
          // Render cursor on the correct column
          const before = line.slice(0, cursorCol);
          const cursorChar = line[cursorCol] ?? ' ';
          const after = line.slice(cursorCol + 1);
          return (
            <Text key={row}>
              {before}
              {chalk.inverse(cursorChar)}
              {after}
            </Text>
          );
        })}
      </Box>
      <Text>{hint}</Text>
    </Box>
  );
}

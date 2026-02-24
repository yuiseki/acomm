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
 *   Tab                 — trigger slash command autocomplete (if dropdown is open)
 *   Ctrl+P              — history up
 *   Ctrl+N              — history down
 *
 * Japanese / CJK support:
 *   Each full-width character occupies 2 terminal columns. This component
 *   manually wraps input text at the available column width using string-width
 *   so that Japanese input never causes horizontal overflow.
 */

import React, { useCallback, useEffect, useRef, useState } from 'react';
import { Box, Text, useInput, getInnerWidth, type DOMElement } from 'ink';
import chalk from 'chalk';
import {
  insertAt,
  deleteCharBefore,
  offsetToRowCol,
  rowColToOffset,
  wrapLine,
} from './textHelpers.js';
import { normalizeInsertedInput, shouldInsertNewline } from './inputKeyHelpers.js';

interface Props {
  value: string;
  cursorOffset: number; // UTF-16 code-unit offset into value
  isProcessing: boolean;
  activeTool: string;
  terminalWidth?: number;
  isActive?: boolean; // when false, input handling is suspended (e.g. during menu mode)
  onChangeCursor: (offset: number) => void;
  onChangeValue: (value: string, cursor: number) => void;
  onSubmit: (value: string) => void;
  onHistoryUp: () => void;
  onHistoryDown: () => void;
  /** Called when Tab is pressed; used to confirm a slash command autocomplete. */
  onTabComplete?: () => void;
  /** When true, show "Tab=complete" hint instead of history hint. */
  hasCompletions?: boolean;
}

export default function MultilineInput({
  value,
  cursorOffset,
  isProcessing,
  activeTool,
  terminalWidth,
  isActive = true,
  onChangeCursor,
  onChangeValue,
  onSubmit,
  onHistoryUp,
  onHistoryDown,
  onTabComplete,
  hasCompletions = false,
}: Props): React.JSX.Element {
  const rootRef = useRef<DOMElement | null>(null);
  const [measuredTextWidth, setMeasuredTextWidth] = useState<number | null>(null);
  const ignoreNextBareLineFeedRef = useRef(false);

  const handleInput = useCallback(
    (input: string, key: ReturnType<typeof useInput extends (h: (i: string, k: infer K) => void) => void ? never : never> extends never ? any : any) => {
      if (isProcessing) return;

      // Some terminals send Shift+Enter as a bare LF (`\n`) with no modifier flags.
      // IME commit flows can also emit a bare LF, so we selectively ignore the next
      // bare LF after multi-char / IME-like inserts (see regular input branch below).
      const isBareLineFeed = input === '\n' && !key.return && !key.ctrl;
      if (isBareLineFeed && ignoreNextBareLineFeedRef.current) {
        ignoreNextBareLineFeedRef.current = false;
        return;
      }

      // --- newline: Shift+Enter, Alt+Enter, Ctrl+J ---
      const isNewline = shouldInsertNewline(input, key, {
        allowBareLineFeedFallback: isBareLineFeed,
      }); // Includes bare-LF Shift+Enter fallback with IME guard above.

      if (isNewline) {
        ignoreNextBareLineFeedRef.current = false;
        const next = insertAt(value, cursorOffset, '\n');
        onChangeValue(next, cursorOffset + 1);
        return;
      }

      // --- submit: plain Enter ---
      if (key.return) {
        ignoreNextBareLineFeedRef.current = false;
        if (value.trim()) onSubmit(value);
        return;
      }

      // --- backspace ---
      if (key.backspace || key.delete) {
        ignoreNextBareLineFeedRef.current = false;
        const result = deleteCharBefore(value, cursorOffset);
        onChangeValue(result.text, result.cursor);
        return;
      }

      // --- cursor movement ---
      if (key.leftArrow) {
        ignoreNextBareLineFeedRef.current = false;
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
        ignoreNextBareLineFeedRef.current = false;
        if (cursorOffset < value.length) {
          // Move forward one code point
          const rest = value.slice(cursorOffset);
          const nextCp = Array.from(rest)[0] ?? '';
          onChangeCursor(cursorOffset + nextCp.length);
        }
        return;
      }
      if (key.upArrow) {
        ignoreNextBareLineFeedRef.current = false;
        const [row, col] = offsetToRowCol(value, cursorOffset);
        if (row > 0) onChangeCursor(rowColToOffset(value, row - 1, col));
        return;
      }
      if (key.downArrow) {
        ignoreNextBareLineFeedRef.current = false;
        const [row, col] = offsetToRowCol(value, cursorOffset);
        const lines = value.split('\n');
        if (row < lines.length - 1) onChangeCursor(rowColToOffset(value, row + 1, col));
        return;
      }

      // Ctrl+A / Home — start of current line
      if ((key.ctrl && input === 'a') || key.home) {
        ignoreNextBareLineFeedRef.current = false;
        const [row] = offsetToRowCol(value, cursorOffset);
        onChangeCursor(rowColToOffset(value, row, 0));
        return;
      }
      // Ctrl+E / End — end of current line
      if ((key.ctrl && input === 'e') || key.end) {
        ignoreNextBareLineFeedRef.current = false;
        const [row] = offsetToRowCol(value, cursorOffset);
        const lineLen = value.split('\n')[row]?.length ?? 0;
        onChangeCursor(rowColToOffset(value, row, lineLen));
        return;
      }

      // Ctrl+P — history up
      if (key.ctrl && input === 'p') {
        ignoreNextBareLineFeedRef.current = false;
        onHistoryUp();
        return;
      }
      // Ctrl+N — history down
      if (key.ctrl && input === 'n') {
        ignoreNextBareLineFeedRef.current = false;
        onHistoryDown();
        return;
      }

      // Escape — ignore
      if (key.escape) {
        ignoreNextBareLineFeedRef.current = false;
        return;
      }

      // --- Tab — trigger slash command autocomplete ---
      if (input === '\t') {
        ignoreNextBareLineFeedRef.current = false;
        onTabComplete?.();
        return;
      }

      // --- regular character input ---
      if (!key.ctrl && !key.meta && input) {
        const insertText = normalizeInsertedInput(input);
        if (!insertText) return;
        // IME commits and some paste payloads often arrive as multi-char insertions.
        // Arm a one-shot bare-LF ignore to avoid treating a follow-up IME Enter as Shift+Enter.
        ignoreNextBareLineFeedRef.current =
          input !== insertText || Array.from(insertText).length > 1;
        const next = insertAt(value, cursorOffset, insertText);
        onChangeValue(next, cursorOffset + insertText.length);
      }
    },
    [value, cursorOffset, isProcessing, onChangeCursor, onChangeValue, onSubmit, onHistoryUp, onHistoryDown, onTabComplete],
  );

  useInput(handleInput as any, { isActive });

  // ---------------------------------------------------------------------------
  // Visual rendering with full-width (CJK) aware line wrapping
  // ---------------------------------------------------------------------------

  // Available text width: terminal columns minus border(2) + paddingLeft(1) + safety(1)
  const containerWidth = Math.max(20, terminalWidth ?? process.stdout.columns ?? 80);
  const fallbackTextWidth = Math.max(20, containerWidth - 4);

  // Measure the actual rendered width after Ink layout settles (initial mount can be off).
  // This fixes first-input wrapping glitches when the first frame uses a transient width.
  useEffect(() => {
    const updateMeasuredWidth = () => {
      if (!rootRef.current) return;
      try {
        // Root inner width excludes border; subtract left padding (1) for text area width.
        const innerWidth = getInnerWidth(rootRef.current);
        const textWidth = Math.max(1, innerWidth - 1);
        setMeasuredTextWidth((prev) => (prev === textWidth ? prev : textWidth));
      } catch {
        // non-fatal; fallbackTextWidth remains in use
      }
    };

    // Measure immediately and on the next tick(s) to catch initial layout stabilization.
    updateMeasuredWidth();
    const t1 = setTimeout(updateMeasuredWidth, 0);
    const t2 = setTimeout(updateMeasuredWidth, 16);

    const stdoutAny = process.stdout as NodeJS.WriteStream & { on?: (ev: string, cb: () => void) => void; off?: (ev: string, cb: () => void) => void };
    stdoutAny.on?.('resize', updateMeasuredWidth);

    return () => {
      clearTimeout(t1);
      clearTimeout(t2);
      stdoutAny.off?.('resize', updateMeasuredWidth);
    };
  }, [containerWidth]);

  const inputWidth = measuredTextWidth ?? fallbackTextWidth;

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
          <Box key={key} height={1} width={inputWidth} overflow="hidden">
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
          <Box key={key} height={1} width={inputWidth} overflow="hidden">
            {before ? <Text>{before}</Text> : null}
            <Text inverse>{cursorChar}</Text>
            {after ? <Text>{after}</Text> : null}
          </Box>,
        );
      } else {
        visualRows.push(
          <Box key={key} height={1} width={inputWidth} overflow="hidden">
            <Text>{chunk.text}</Text>
          </Box>,
        );
      }
    });
  });

  return (
    <Box ref={rootRef} flexDirection="column" width={containerWidth} borderStyle="single" borderColor={isProcessing ? 'yellow' : 'cyan'}>
      <Box flexDirection="column" width={containerWidth - 2} paddingLeft={1}>
        {visualRows}
      </Box>
    </Box>
  );
}

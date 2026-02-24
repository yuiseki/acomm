/**
 * Pure text-manipulation helpers used by MultilineInput.
 * Kept in a separate module so they can be unit-tested without importing React/Ink.
 *
 * All cursor offsets are UTF-16 code-unit offsets (i.e. String.prototype indices),
 * which matches how JavaScript strings work. For BMP characters (including all
 * Japanese kana/kanji), a code unit == a code point == 1 character.
 */

import ansiRegex from 'ansi-regex';
import stringWidth from 'string-width';
import stripAnsi from 'strip-ansi';

// ---------------------------------------------------------------------------
// Basic string operations (cursor-offset aware)
// ---------------------------------------------------------------------------

/** Insert a string at a UTF-16 code-unit offset inside text. */
export function insertAt(text: string, offset: number, insertion: string): string {
  return text.slice(0, offset) + insertion + text.slice(offset);
}

/**
 * Delete the Unicode code point immediately before the cursor.
 * Uses Array.from() to split by code points (handles surrogate-pair emoji, etc.).
 */
export function deleteCharBefore(text: string, offset: number): { text: string; cursor: number } {
  if (offset === 0) return { text, cursor: 0 };
  const before = text.slice(0, offset);
  const cps = Array.from(before);
  if (cps.length === 0) return { text, cursor: 0 };
  const newBefore = cps.slice(0, -1).join('');
  return { text: newBefore + text.slice(offset), cursor: newBefore.length };
}

/** Compute (row, col) from a flat UTF-16 offset in text. col is a UTF-16 unit count. */
export function offsetToRowCol(text: string, offset: number): [number, number] {
  const before = text.slice(0, offset);
  const lines = before.split('\n');
  return [lines.length - 1, lines[lines.length - 1]!.length];
}

/** Compute the flat UTF-16 offset from (row, col) in text. col is a UTF-16 unit count. */
export function rowColToOffset(text: string, row: number, col: number): number {
  const lines = text.split('\n');
  let offset = 0;
  for (let i = 0; i < row && i < lines.length; i++) {
    offset += lines[i]!.length + 1; // +1 for '\n'
  }
  const lineLen = lines[row]?.length ?? 0;
  return offset + Math.min(col, lineLen);
}

// ---------------------------------------------------------------------------
// Unicode / visual-width helpers  (gemini-cli textUtils.ts を参考)
// ---------------------------------------------------------------------------

/** Split a string into Unicode code points (handles surrogate pairs). */
export function toCodePoints(str: string): string[] {
  return Array.from(str);
}

/** Length in Unicode code points. */
export function cpLen(str: string): number {
  return Array.from(str).length;
}

/**
 * Slice by code point index.
 * Unlike String.prototype.slice (which operates on UTF-16 units),
 * this correctly handles surrogate-pair characters like emoji.
 */
export function cpSlice(str: string, start: number, end?: number): string {
  return Array.from(str).slice(start, end).join('');
}

/**
 * Visual terminal column width of a string.
 * Full-width CJK characters count as 2 columns; ASCII counts as 1.
 */
export function visualWidth(str: string): number {
  return stringWidth(stripAnsi(str));
}

// ---------------------------------------------------------------------------
// Visual line wrapping
// ---------------------------------------------------------------------------

/** A segment of a logical line that fits within maxWidth terminal columns. */
export interface VisualChunk {
  /** The text of this visual segment. */
  text: string;
  /** Code-point index of the first character within the original logical line. */
  startCpIdx: number;
  /** Code-point index past the last character (exclusive). */
  endCpIdx: number;
}

interface AnsiToken {
  kind: 'ansi' | 'text';
  value: string;
}

const SGR_RESET = '\x1b[0m';
const ESC = '\x1b[';
const C1_CSI = '\u009b';

function tokenizeAnsi(input: string): AnsiToken[] {
  const tokens: AnsiToken[] = [];
  let last = 0;

  for (const match of input.matchAll(ansiRegex())) {
    const idx = match.index ?? 0;
    if (idx > last) {
      tokens.push({ kind: 'text', value: input.slice(last, idx) });
    }
    tokens.push({ kind: 'ansi', value: match[0] });
    last = idx + match[0].length;
  }

  if (last < input.length) {
    tokens.push({ kind: 'text', value: input.slice(last) });
  }

  return tokens;
}

function isSgrSequence(seq: string): boolean {
  return (seq.startsWith(ESC) || seq.startsWith(C1_CSI)) && seq.endsWith('m');
}

function sgrHasReset(seq: string): boolean {
  if (!isSgrSequence(seq)) return false;
  const body = seq.startsWith(ESC) ? seq.slice(2, -1) : seq.slice(1, -1);
  if (body === '') return true; // ESC[m == reset
  return body.split(';').some((p) => p === '' || p === '0');
}

function isPureResetSgr(seq: string): boolean {
  if (!isSgrSequence(seq)) return false;
  const body = seq.startsWith(ESC) ? seq.slice(2, -1) : seq.slice(1, -1);
  return body === '' || body === '0';
}

function nextActiveSgr(activeSgr: string, seq: string): string {
  if (!isSgrSequence(seq)) return activeSgr;
  const reset = sgrHasReset(seq);
  const next = reset ? '' : activeSgr;
  if (isPureResetSgr(seq)) return next;
  return next + seq;
}

/**
 * Wraps a single logical line into VisualChunks that each fit within
 * maxWidth terminal columns, accounting for full-width (CJK) characters.
 *
 * Always returns at least one chunk, even for empty lines.
 */
export function wrapLine(line: string, maxWidth: number): VisualChunk[] {
  const visibleCodePoints = Array.from(stripAnsi(line));

  if (visibleCodePoints.length === 0 || maxWidth <= 0) {
    return [{ text: line, startCpIdx: 0, endCpIdx: visibleCodePoints.length }];
  }

  const tokens = tokenizeAnsi(line);
  const chunks: VisualChunk[] = [];

  let chunkText = '';
  let chunkStart = 0;
  let currentWidth = 0;
  let visibleCpIdx = 0;
  let activeSgr = '';

  const flushChunk = () => {
    let text = chunkText;
    if (activeSgr && text.length > 0 && !text.endsWith(SGR_RESET)) {
      text += SGR_RESET;
    }
    chunks.push({
      text,
      startCpIdx: chunkStart,
      endCpIdx: visibleCpIdx,
    });
  };

  for (const token of tokens) {
    if (token.kind === 'ansi') {
      chunkText += token.value;
      activeSgr = nextActiveSgr(activeSgr, token.value);
      continue;
    }

    for (const cp of Array.from(token.value)) {
      const charWidth = visualWidth(cp);
      if (currentWidth + charWidth > maxWidth && currentWidth > 0) {
        flushChunk();
        chunkStart = visibleCpIdx;
        chunkText = activeSgr;
        currentWidth = 0;
      }
      chunkText += cp;
      currentWidth += charWidth;
      visibleCpIdx += 1;
    }
  }

  flushChunk();
  return chunks;
}

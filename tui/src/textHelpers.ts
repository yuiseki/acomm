/**
 * Pure text-manipulation helpers used by MultilineInput.
 * Kept in a separate module so they can be unit-tested without importing React/Ink.
 *
 * All cursor offsets are UTF-16 code-unit offsets (i.e. String.prototype indices),
 * which matches how JavaScript strings work. For BMP characters (including all
 * Japanese kana/kanji), a code unit == a code point == 1 character.
 */

import stringWidth from 'string-width';

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
  return stringWidth(str);
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

/**
 * Wraps a single logical line into VisualChunks that each fit within
 * maxWidth terminal columns, accounting for full-width (CJK) characters.
 *
 * Always returns at least one chunk, even for empty lines.
 */
export function wrapLine(line: string, maxWidth: number): VisualChunk[] {
  const codePoints = Array.from(line);

  if (codePoints.length === 0 || maxWidth <= 0) {
    return [{ text: line, startCpIdx: 0, endCpIdx: codePoints.length }];
  }

  const chunks: VisualChunk[] = [];
  let chunkStart = 0;
  let currentWidth = 0;

  for (let i = 0; i < codePoints.length; i++) {
    const charWidth = stringWidth(codePoints[i]!);

    if (currentWidth + charWidth > maxWidth && currentWidth > 0) {
      // Flush current chunk before this character overflows
      chunks.push({
        text: codePoints.slice(chunkStart, i).join(''),
        startCpIdx: chunkStart,
        endCpIdx: i,
      });
      chunkStart = i;
      currentWidth = 0;
    }
    currentWidth += charWidth;
  }

  // Flush remainder
  chunks.push({
    text: codePoints.slice(chunkStart).join(''),
    startCpIdx: chunkStart,
    endCpIdx: codePoints.length,
  });

  return chunks;
}

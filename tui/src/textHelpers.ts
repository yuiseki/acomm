/**
 * Pure text-manipulation helpers used by MultilineInput.
 * Kept in a separate module so they can be unit-tested without importing React/Ink.
 */

/** Insert a string at a byte offset inside text. */
export function insertAt(text: string, offset: number, insertion: string): string {
  return text.slice(0, offset) + insertion + text.slice(offset);
}

/** Delete one character before the byte offset (handles multi-byte UTF-8). */
export function deleteCharBefore(text: string, offset: number): { text: string; cursor: number } {
  if (offset === 0) return { text, cursor: 0 };
  // Walk backwards to find the previous code-point boundary
  let i = offset - 1;
  while (i > 0 && (text.charCodeAt(i) & 0xc0) === 0x80) i--;
  return { text: text.slice(0, i) + text.slice(offset), cursor: i };
}

/** Compute (row, col) from a flat byte offset in text. */
export function offsetToRowCol(text: string, offset: number): [number, number] {
  const before = text.slice(0, offset);
  const lines = before.split('\n');
  return [lines.length - 1, lines[lines.length - 1]!.length];
}

/** Compute the flat byte offset from (row, col) in text. */
export function rowColToOffset(text: string, row: number, col: number): number {
  const lines = text.split('\n');
  let offset = 0;
  for (let i = 0; i < row && i < lines.length; i++) {
    offset += lines[i]!.length + 1; // +1 for '\n'
  }
  const lineLen = lines[row]?.length ?? 0;
  return offset + Math.min(col, lineLen);
}

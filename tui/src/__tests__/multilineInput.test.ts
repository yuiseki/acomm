import { describe, it, expect } from 'vitest';
import {
  insertAt,
  deleteCharBefore,
  offsetToRowCol,
  rowColToOffset,
  toCodePoints,
  cpLen,
  cpSlice,
  visualWidth,
  wrapLine,
} from '../textHelpers.js';

// ---------------------------------------------------------------------------
// insertAt
// ---------------------------------------------------------------------------

describe('insertAt', () => {
  it('inserts in the middle of a string', () => {
    expect(insertAt('hello', 2, 'XY')).toBe('heXYllo');
  });

  it('inserts at the start', () => {
    expect(insertAt('hello', 0, 'A')).toBe('Ahello');
  });

  it('inserts at the end', () => {
    expect(insertAt('hello', 5, '!')).toBe('hello!');
  });

  it('inserts a newline', () => {
    expect(insertAt('ab', 1, '\n')).toBe('a\nb');
  });

  it('is a no-op with an empty insertion', () => {
    expect(insertAt('hello', 2, '')).toBe('hello');
  });

  it('inserts a Japanese character', () => {
    expect(insertAt('あい', 1, 'う')).toBe('あうい');
  });
});

// ---------------------------------------------------------------------------
// deleteCharBefore
// ---------------------------------------------------------------------------

describe('deleteCharBefore', () => {
  it('deletes the character immediately before the cursor', () => {
    const r = deleteCharBefore('hello', 3);
    expect(r.text).toBe('helo');
    expect(r.cursor).toBe(2);
  });

  it('is a no-op when cursor is at position 0', () => {
    const r = deleteCharBefore('hello', 0);
    expect(r.text).toBe('hello');
    expect(r.cursor).toBe(0);
  });

  it('deletes the last character', () => {
    const r = deleteCharBefore('hello', 5);
    expect(r.text).toBe('hell');
    expect(r.cursor).toBe(4);
  });

  it('handles deleting a newline', () => {
    const r = deleteCharBefore('a\nb', 2); // cursor after '\n'
    expect(r.text).toBe('ab');
    expect(r.cursor).toBe(1);
  });

  it('deletes a Japanese character', () => {
    const r = deleteCharBefore('こんにちは', 3); // cursor after 'こんに'
    expect(r.text).toBe('こんちは');
    expect(r.cursor).toBe(2);
  });

  it('deletes a full-width character at the end', () => {
    const r = deleteCharBefore('日本語', 3);
    expect(r.text).toBe('日本');
    expect(r.cursor).toBe(2);
  });
});

// ---------------------------------------------------------------------------
// offsetToRowCol
// ---------------------------------------------------------------------------

describe('offsetToRowCol', () => {
  const text = 'line0\nline1\nline2';

  it('returns [0, 0] for offset 0', () => {
    expect(offsetToRowCol(text, 0)).toEqual([0, 0]);
  });

  it('returns correct col on first line', () => {
    expect(offsetToRowCol(text, 3)).toEqual([0, 3]);
  });

  it('returns [1, 0] at start of second line', () => {
    expect(offsetToRowCol(text, 6)).toEqual([1, 0]);
  });

  it('returns correct row+col inside second line', () => {
    expect(offsetToRowCol(text, 7)).toEqual([1, 1]);
  });

  it('returns [2, 0] at start of third line', () => {
    expect(offsetToRowCol(text, 12)).toEqual([2, 0]);
  });
});

// ---------------------------------------------------------------------------
// rowColToOffset
// ---------------------------------------------------------------------------

describe('rowColToOffset', () => {
  const text = 'line0\nline1\nline2';

  it('returns 0 for (0, 0)', () => {
    expect(rowColToOffset(text, 0, 0)).toBe(0);
  });

  it('returns correct offset for (0, 3)', () => {
    expect(rowColToOffset(text, 0, 3)).toBe(3);
  });

  it('returns 6 for start of second line', () => {
    expect(rowColToOffset(text, 1, 0)).toBe(6);
  });

  it('clamps col to line length', () => {
    expect(rowColToOffset(text, 0, 99)).toBe(5);
  });
});

// ---------------------------------------------------------------------------
// offsetToRowCol / rowColToOffset round-trip
// ---------------------------------------------------------------------------

describe('offsetToRowCol / rowColToOffset round-trip', () => {
  const text = 'first\nsecond\nthird';

  it('round-trips correctly for various offsets', () => {
    for (let i = 0; i <= text.length; i++) {
      const [row, col] = offsetToRowCol(text, i);
      const back = rowColToOffset(text, row, col);
      expect(back).toBe(i);
    }
  });
});

// ---------------------------------------------------------------------------
// toCodePoints / cpLen / cpSlice
// ---------------------------------------------------------------------------

describe('toCodePoints', () => {
  it('splits ASCII', () => {
    expect(toCodePoints('abc')).toEqual(['a', 'b', 'c']);
  });

  it('splits Japanese (BMP)', () => {
    expect(toCodePoints('こんにちは')).toEqual(['こ', 'ん', 'に', 'ち', 'は']);
  });

  it('returns empty array for empty string', () => {
    expect(toCodePoints('')).toEqual([]);
  });
});

describe('cpLen', () => {
  it('returns correct length for ASCII', () => {
    expect(cpLen('hello')).toBe(5);
  });

  it('returns correct length for Japanese', () => {
    expect(cpLen('こんにちは')).toBe(5);
  });

  it('returns 0 for empty string', () => {
    expect(cpLen('')).toBe(0);
  });
});

describe('cpSlice', () => {
  it('slices ASCII correctly', () => {
    expect(cpSlice('hello', 1, 3)).toBe('el');
  });

  it('slices Japanese correctly', () => {
    expect(cpSlice('こんにちは', 1, 3)).toBe('んに');
  });

  it('handles open-ended slice', () => {
    expect(cpSlice('こんにちは', 2)).toBe('にちは');
  });
});

// ---------------------------------------------------------------------------
// visualWidth
// ---------------------------------------------------------------------------

describe('visualWidth', () => {
  it('ASCII chars have width 1 each', () => {
    expect(visualWidth('hello')).toBe(5);
  });

  it('full-width Japanese chars have width 2 each', () => {
    expect(visualWidth('こんにちは')).toBe(10);
  });

  it('mixed string has correct visual width', () => {
    // 'AB' = 2, '日本' = 4 → total 6
    expect(visualWidth('AB日本')).toBe(6);
  });

  it('empty string has width 0', () => {
    expect(visualWidth('')).toBe(0);
  });
});

// ---------------------------------------------------------------------------
// wrapLine
// ---------------------------------------------------------------------------

describe('wrapLine', () => {
  it('returns one chunk for short ASCII (fits within maxWidth)', () => {
    const chunks = wrapLine('hello', 80);
    expect(chunks).toHaveLength(1);
    expect(chunks[0]!.text).toBe('hello');
    expect(chunks[0]!.startCpIdx).toBe(0);
    expect(chunks[0]!.endCpIdx).toBe(5);
  });

  it('wraps ASCII at maxWidth boundary', () => {
    // 'helloabcde' = 10 chars; maxWidth=5 → 'hello'(5) + 'abcde'(5)
    const chunks = wrapLine('helloabcde', 5);
    expect(chunks).toHaveLength(2);
    expect(chunks[0]!.text).toBe('hello');
    expect(chunks[0]!.startCpIdx).toBe(0);
    expect(chunks[0]!.endCpIdx).toBe(5);
    expect(chunks[1]!.text).toBe('abcde');
    expect(chunks[1]!.startCpIdx).toBe(5);
    expect(chunks[1]!.endCpIdx).toBe(10);
  });

  it('wraps Japanese at correct visual boundary (each char = 2 cols)', () => {
    // 'こんにちは' = 5 chars × 2 cols = 10 visual cols
    // maxWidth=6 → 'こんに'(6 cols) + 'ちは'(4 cols)
    const chunks = wrapLine('こんにちは', 6);
    expect(chunks).toHaveLength(2);
    expect(chunks[0]!.text).toBe('こんに');
    expect(chunks[0]!.startCpIdx).toBe(0);
    expect(chunks[0]!.endCpIdx).toBe(3);
    expect(chunks[1]!.text).toBe('ちは');
    expect(chunks[1]!.startCpIdx).toBe(3);
    expect(chunks[1]!.endCpIdx).toBe(5);
  });

  it('wraps mixed ASCII + Japanese correctly', () => {
    // 'AB日本' = A(1)+B(1)+日(2)+本(2) = 6 cols; maxWidth=4 → 'AB日'(4) + '本'(2)
    const chunks = wrapLine('AB日本', 4);
    expect(chunks).toHaveLength(2);
    expect(chunks[0]!.text).toBe('AB日');
    expect(chunks[0]!.endCpIdx).toBe(3);
    expect(chunks[1]!.text).toBe('本');
    expect(chunks[1]!.startCpIdx).toBe(3);
  });

  it('handles empty line', () => {
    const chunks = wrapLine('', 80);
    expect(chunks).toHaveLength(1);
    expect(chunks[0]!.text).toBe('');
    expect(chunks[0]!.startCpIdx).toBe(0);
    expect(chunks[0]!.endCpIdx).toBe(0);
  });

  it('never produces chunks that exceed maxWidth (Japanese)', () => {
    const line = 'あいうえおかきくけこさしすせそ'; // 15 chars × 2 = 30 cols
    const maxWidth = 10;
    const chunks = wrapLine(line, maxWidth);
    for (const chunk of chunks) {
      expect(visualWidth(chunk.text)).toBeLessThanOrEqual(maxWidth);
    }
  });

  it('covers the entire line across all chunks', () => {
    const line = 'こんにちはABC世界';
    const chunks = wrapLine(line, 8);
    const reconstructed = chunks.map((c) => c.text).join('');
    expect(reconstructed).toBe(line);
  });

  it('chunk code-point indices are contiguous and cover [0, cpLen)', () => {
    const line = '日本語テスト';
    const chunks = wrapLine(line, 6);
    expect(chunks[0]!.startCpIdx).toBe(0);
    for (let i = 1; i < chunks.length; i++) {
      expect(chunks[i]!.startCpIdx).toBe(chunks[i - 1]!.endCpIdx);
    }
    expect(chunks[chunks.length - 1]!.endCpIdx).toBe(cpLen(line));
  });
});

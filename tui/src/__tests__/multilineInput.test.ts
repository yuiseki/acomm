import { describe, it, expect } from 'vitest';
import {
  insertAt,
  deleteCharBefore,
  offsetToRowCol,
  rowColToOffset,
} from '../textHelpers.js';

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
});

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
});

describe('offsetToRowCol', () => {
  const text = 'line0\nline1\nline2';

  it('returns [0, 0] for offset 0', () => {
    expect(offsetToRowCol(text, 0)).toEqual([0, 0]);
  });

  it('returns correct col on first line', () => {
    expect(offsetToRowCol(text, 3)).toEqual([0, 3]);
  });

  it('returns [1, 0] at start of second line', () => {
    // "line0\n" = 6 chars, so offset 6 is start of line 1
    expect(offsetToRowCol(text, 6)).toEqual([1, 0]);
  });

  it('returns correct row+col inside second line', () => {
    // offset 7 = second line col 1
    expect(offsetToRowCol(text, 7)).toEqual([1, 1]);
  });

  it('returns [2, 0] at start of third line', () => {
    expect(offsetToRowCol(text, 12)).toEqual([2, 0]);
  });
});

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
    // line0 is 5 chars; col 99 should clamp to 5
    expect(rowColToOffset(text, 0, 99)).toBe(5);
  });
});

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

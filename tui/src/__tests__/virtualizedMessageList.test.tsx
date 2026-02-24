import { describe, expect, it } from 'vitest';
import type { VMessage } from '../VirtualizedMessageList.js';
import { flattenMessages } from '../VirtualizedMessageList.js';

function msg(text: string, overrides?: Partial<VMessage>): VMessage {
  return {
    id: 1,
    prefix: '',
    text,
    isAgent: false,
    isStreaming: false,
    ...overrides,
  };
}

describe('flattenMessages', () => {
  it('wraps long CJK lines by terminal width when maxWidth is provided', () => {
    const lines = flattenMessages([msg('こんにちは')], false, 0, 4);
    expect(lines).toEqual(['こん', 'にち', 'は']);
  });

  it('wraps ANSI-colored lines by visible width when maxWidth is provided', () => {
    const red = '\x1b[31m';
    const reset = '\x1b[0m';
    const lines = flattenMessages([msg(`${red}AB日本${reset}`)], false, 0, 4);
    const stripSgr = (s: string) => s.replace(/\x1b\[[0-9;]*m/g, '');

    expect(lines).toHaveLength(2);
    expect(stripSgr(lines[0]!)).toBe('AB日');
    expect(stripSgr(lines[1]!)).toBe('本');
  });

  it('preserves explicit newlines before applying visual wrapping', () => {
    const lines = flattenMessages([msg('ABCDE\nFGHIJ')], false, 0, 3);
    expect(lines).toEqual(['ABC', 'DE', 'FGH', 'IJ']);
  });
});

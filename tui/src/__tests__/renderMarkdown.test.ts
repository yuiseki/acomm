import { describe, it, expect } from 'vitest';
import { renderMarkdown } from '../renderMarkdown.js';

describe('renderMarkdown', () => {
  it('returns empty string for empty input', () => {
    expect(renderMarkdown('')).toBe('');
  });

  it('returns whitespace-only input as-is', () => {
    const result = renderMarkdown('   ');
    expect(result.trim()).toBe('');
  });

  it('preserves plain text content', () => {
    const result = renderMarkdown('hello world');
    expect(result).toContain('hello world');
  });

  it('renders bold text containing the original words', () => {
    const result = renderMarkdown('**bold text**');
    expect(result).toContain('bold text');
  });

  it('renders italic text containing the original words', () => {
    const result = renderMarkdown('*italic text*');
    expect(result).toContain('italic text');
  });

  it('renders inline code containing the code', () => {
    const result = renderMarkdown('Use `console.log()` here');
    expect(result).toContain('console.log()');
  });

  it('renders fenced code block containing the code', () => {
    const result = renderMarkdown('```js\nconsole.log("hello");\n```');
    expect(result).toContain('console.log');
  });

  it('renders h1 heading containing the heading text', () => {
    const result = renderMarkdown('# My Title');
    expect(result).toContain('My Title');
  });

  it('renders h2 heading containing the heading text', () => {
    const result = renderMarkdown('## Section');
    expect(result).toContain('Section');
  });

  it('renders unordered list items', () => {
    const result = renderMarkdown('- item one\n- item two');
    expect(result).toContain('item one');
    expect(result).toContain('item two');
  });

  it('renders ordered list items', () => {
    const result = renderMarkdown('1. first\n2. second');
    expect(result).toContain('first');
    expect(result).toContain('second');
  });

  it('renders blockquotes containing the text', () => {
    const result = renderMarkdown('> quoted text');
    expect(result).toContain('quoted text');
  });

  it('preserves CJK (Japanese) text', () => {
    const result = renderMarkdown('おはようございます！');
    expect(result).toContain('おはようございます');
  });

  it('preserves CJK in code blocks', () => {
    const result = renderMarkdown('```\n日本語コード\n```');
    expect(result).toContain('日本語コード');
  });

  it('returns a string type', () => {
    const result = renderMarkdown('# test\n\nsome text');
    expect(typeof result).toBe('string');
  });
});

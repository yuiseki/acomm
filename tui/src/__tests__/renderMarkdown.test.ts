import { describe, it, expect } from 'vitest';
import { supportsLanguage } from 'cli-highlight';
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

  it('does not append trailing newlines for plain text', () => {
    const result = renderMarkdown('hello world');
    expect(result.endsWith('\n')).toBe(false);
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

  it('does not leave trailing blank lines after fenced code blocks', () => {
    const result = renderMarkdown('```js\nconsole.log("hello");\n```');
    expect(result.endsWith('\n')).toBe(false);
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

  // --- Syntax Highlighting (cli-highlight integration) ---

  it('code block without language tag preserves the source code', () => {
    const result = renderMarkdown('```\nsome plain code\n```');
    expect(result).toContain('some plain code');
  });

  it('javascript code block preserves identifiers', () => {
    const result = renderMarkdown('```javascript\nfunction add(a, b) { return a + b; }\n```');
    expect(result).toContain('add');
    expect(result).toContain('return');
  });

  it('typescript code block preserves type annotations', () => {
    const result = renderMarkdown('```typescript\nconst x: number = 42;\n```');
    expect(result).toContain('x');
    expect(result).toContain('42');
  });

  it('python code block preserves python keywords', () => {
    const result = renderMarkdown('```python\ndef greet(name):\n    return f"hello {name}"\n```');
    expect(result).toContain('greet');
    expect(result).toContain('return');
  });

  it('rust code block preserves rust syntax', () => {
    const result = renderMarkdown('```rust\nfn main() { println!("hello"); }\n```');
    expect(result).toContain('main');
    expect(result).toContain('println');
  });

  it('code block is indented by 4 spaces', () => {
    const result = renderMarkdown('```\nhello\n```');
    // Each line in the code block should be prefixed with 4 spaces
    const lines = result.split('\n').filter((l) => l.includes('hello'));
    expect(lines.length).toBeGreaterThan(0);
    expect(lines[0]).toMatch(/^ {4}/);
  });

  it('supportsLanguage returns true for common languages', () => {
    expect(supportsLanguage('javascript')).toBe(true);
    expect(supportsLanguage('typescript')).toBe(true);
    expect(supportsLanguage('python')).toBe(true);
    expect(supportsLanguage('rust')).toBe(true);
    expect(supportsLanguage('bash')).toBe(true);
  });

  it('supportsLanguage returns false for unknown language', () => {
    expect(supportsLanguage('notareallanguage12345')).toBe(false);
  });

  it('unknown language falls back to plain text without error', () => {
    const result = renderMarkdown('```notareallanguage\nsome code\n```');
    expect(result).toContain('some code');
  });
});

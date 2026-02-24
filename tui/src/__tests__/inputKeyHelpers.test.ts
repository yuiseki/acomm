import { describe, expect, it } from 'vitest';
import { isTabCompleteTrigger, normalizeInsertedInput, shouldInsertNewline } from '../inputKeyHelpers.js';

describe('shouldInsertNewline', () => {
  it('does not treat Shift+Enter as newline insertion (unsupported)', () => {
    expect(shouldInsertNewline('', { return: true, shift: true })).toBe(false);
  });

  it('treats Alt+Enter as newline insertion', () => {
    expect(shouldInsertNewline('', { return: true, alt: true })).toBe(true);
  });

  it('treats Ctrl+J (LF with ctrl) as newline insertion fallback', () => {
    expect(shouldInsertNewline('\n', { ctrl: true, return: false })).toBe(true);
  });

  it('does not treat a bare LF without ctrl as newline insertion (IME-safe)', () => {
    expect(shouldInsertNewline('\n', { return: false, ctrl: false })).toBe(false);
  });

  it('does not treat plain Enter as newline insertion', () => {
    expect(shouldInsertNewline('', { return: true })).toBe(false);
  });
});

describe('normalizeInsertedInput', () => {
  it('keeps normal text unchanged', () => {
    expect(normalizeInsertedInput('hello')).toBe('hello');
    expect(normalizeInsertedInput('長い')).toBe('長い');
  });

  it('drops bare CR/LF payloads (not regular text)', () => {
    expect(normalizeInsertedInput('\n')).toBe('');
    expect(normalizeInsertedInput('\r')).toBe('');
    expect(normalizeInsertedInput('\r\n')).toBe('');
  });

  it('strips trailing newline from IME-like committed text payloads', () => {
    expect(normalizeInsertedInput('長い\n')).toBe('長い');
    expect(normalizeInsertedInput('長い\r')).toBe('長い');
    expect(normalizeInsertedInput('長い\r\n')).toBe('長い');
  });

  it('preserves internal newlines for multiline paste', () => {
    expect(normalizeInsertedInput('A\nB')).toBe('A\nB');
    expect(normalizeInsertedInput('A\nB\n')).toBe('A\nB');
  });
});

describe('isTabCompleteTrigger', () => {
  it('returns true for Ink tab key objects even when input is empty', () => {
    expect(isTabCompleteTrigger('', { tab: true })).toBe(true);
  });

  it('returns true for raw tab character input', () => {
    expect(isTabCompleteTrigger('\t', { tab: false })).toBe(true);
  });

  it('returns false for non-tab input', () => {
    expect(isTabCompleteTrigger('a', { tab: false })).toBe(false);
  });
});

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtempSync, rmSync, readdirSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  makeSessionsDir,
  saveSessionTurn,
  loadRecentTurns,
  type SessionTurn,
} from '../sessionStorage.js';

// Use a temp directory per test run so tests don't affect each other or real state.
let testDir: string;

beforeEach(() => {
  testDir = mkdtempSync(join(tmpdir(), 'acomm-session-test-'));
});

afterEach(() => {
  rmSync(testDir, { recursive: true, force: true });
});

describe('makeSessionsDir', () => {
  it('creates the directory if it does not exist', () => {
    const dir = join(testDir, 'sessions');
    makeSessionsDir(dir);
    const stat = readdirSync(dir);
    expect(stat).toBeDefined();
  });

  it('is idempotent (calling twice does not throw)', () => {
    const dir = join(testDir, 'sessions');
    makeSessionsDir(dir);
    expect(() => makeSessionsDir(dir)).not.toThrow();
  });
});

describe('saveSessionTurn', () => {
  const sessionsDir = () => join(testDir, 'sessions');

  it('creates a JSONL file named YYYY-MM-DD.jsonl', () => {
    const turn: SessionTurn = {
      timestamp: '2026-02-24T10:00:00.000Z',
      provider: 'Gemini',
      model: 'gemini-2.5-flash',
      prompt: 'hello',
      response: 'Hello there!',
    };
    saveSessionTurn(turn, sessionsDir());
    const files = readdirSync(sessionsDir());
    expect(files).toContain('2026-02-24.jsonl');
  });

  it('appends JSON line containing prompt and response', () => {
    const turn: SessionTurn = {
      timestamp: '2026-02-24T10:00:00.000Z',
      provider: 'Claude',
      model: 'claude-sonnet-4-6',
      prompt: 'What is 2+2?',
      response: '4',
    };
    saveSessionTurn(turn, sessionsDir());
    const content = readFileSync(join(sessionsDir(), '2026-02-24.jsonl'), 'utf8');
    const parsed = JSON.parse(content.trim());
    expect(parsed.prompt).toBe('What is 2+2?');
    expect(parsed.response).toBe('4');
    expect(parsed.provider).toBe('Claude');
    expect(parsed.model).toBe('claude-sonnet-4-6');
  });

  it('appends multiple turns to the same file', () => {
    const base: Omit<SessionTurn, 'prompt' | 'response'> = {
      timestamp: '2026-02-24T10:00:00.000Z',
      provider: 'Gemini',
      model: 'gemini-2.5-flash',
    };
    saveSessionTurn({ ...base, prompt: 'first', response: 'r1' }, sessionsDir());
    saveSessionTurn({ ...base, prompt: 'second', response: 'r2' }, sessionsDir());
    const content = readFileSync(join(sessionsDir(), '2026-02-24.jsonl'), 'utf8');
    const lines = content.trim().split('\n').filter(Boolean);
    expect(lines).toHaveLength(2);
    expect(JSON.parse(lines[0]!).prompt).toBe('first');
    expect(JSON.parse(lines[1]!).prompt).toBe('second');
  });

  it('creates separate files for different dates', () => {
    saveSessionTurn(
      { timestamp: '2026-02-23T10:00:00.000Z', provider: 'Gemini', model: 'g', prompt: 'a', response: 'b' },
      sessionsDir(),
    );
    saveSessionTurn(
      { timestamp: '2026-02-24T10:00:00.000Z', provider: 'Gemini', model: 'g', prompt: 'c', response: 'd' },
      sessionsDir(),
    );
    const files = readdirSync(sessionsDir()).sort();
    expect(files).toContain('2026-02-23.jsonl');
    expect(files).toContain('2026-02-24.jsonl');
  });
});

describe('loadRecentTurns', () => {
  const sessionsDir = () => join(testDir, 'sessions');

  it('returns empty array when sessions directory is empty', () => {
    makeSessionsDir(sessionsDir());
    expect(loadRecentTurns(10, sessionsDir())).toHaveLength(0);
  });

  it('returns empty array when sessions directory does not exist', () => {
    expect(loadRecentTurns(10, join(testDir, 'nonexistent'))).toHaveLength(0);
  });

  it('returns saved turns in order (oldest first)', () => {
    const s = sessionsDir();
    saveSessionTurn({ timestamp: '2026-02-24T09:00:00.000Z', provider: 'Gemini', model: 'g', prompt: 'first', response: 'r1' }, s);
    saveSessionTurn({ timestamp: '2026-02-24T10:00:00.000Z', provider: 'Claude', model: 'c', prompt: 'second', response: 'r2' }, s);
    const turns = loadRecentTurns(10, s);
    expect(turns).toHaveLength(2);
    expect(turns[0]!.prompt).toBe('first');
    expect(turns[1]!.prompt).toBe('second');
  });

  it('respects the limit parameter', () => {
    const s = sessionsDir();
    for (let i = 0; i < 5; i++) {
      saveSessionTurn({ timestamp: `2026-02-24T0${i}:00:00.000Z`, provider: 'Gemini', model: 'g', prompt: `p${i}`, response: `r${i}` }, s);
    }
    const turns = loadRecentTurns(3, s);
    expect(turns).toHaveLength(3);
  });

  it('returns turns with all required fields', () => {
    const s = sessionsDir();
    saveSessionTurn({ timestamp: '2026-02-24T10:00:00.000Z', provider: 'Codex', model: 'gpt-4o', prompt: 'hi', response: 'hello' }, s);
    const turns = loadRecentTurns(10, s);
    expect(turns[0]!.timestamp).toBe('2026-02-24T10:00:00.000Z');
    expect(turns[0]!.provider).toBe('Codex');
    expect(turns[0]!.model).toBe('gpt-4o');
    expect(turns[0]!.prompt).toBe('hi');
    expect(turns[0]!.response).toBe('hello');
  });

  it('skips malformed lines without throwing', () => {
    const s = sessionsDir();
    makeSessionsDir(s);
    // Write a file with one good line and one bad line
    const { appendFileSync } = require('node:fs');
    appendFileSync(join(s, '2026-02-24.jsonl'), 'not valid json\n');
    saveSessionTurn({ timestamp: '2026-02-24T10:00:00.000Z', provider: 'Gemini', model: 'g', prompt: 'good', response: 'ok' }, s);
    const turns = loadRecentTurns(10, s);
    expect(turns).toHaveLength(1);
    expect(turns[0]!.prompt).toBe('good');
  });
});

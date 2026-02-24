import { describe, it, expect } from 'vitest';
import { parseSlashCommand, getSlashCompletions, SLASH_COMMANDS } from '../slashCommands.js';

describe('parseSlashCommand', () => {
  it('returns null for non-slash input', () => {
    expect(parseSlashCommand('hello')).toBeNull();
    expect(parseSlashCommand('')).toBeNull();
    expect(parseSlashCommand('just text')).toBeNull();
  });

  it('returns provider action for /provider', () => {
    expect(parseSlashCommand('/provider')).toEqual({ type: 'provider' });
  });

  it('returns provider action for /PROVIDER (case insensitive)', () => {
    expect(parseSlashCommand('/PROVIDER')).toEqual({ type: 'provider' });
    expect(parseSlashCommand('/Provider')).toEqual({ type: 'provider' });
  });

  it('returns model action for /model', () => {
    expect(parseSlashCommand('/model')).toEqual({ type: 'model' });
  });

  it('returns model action for /MODEL (case insensitive)', () => {
    expect(parseSlashCommand('/MODEL')).toEqual({ type: 'model' });
  });

  it('returns clear action for /clear', () => {
    expect(parseSlashCommand('/clear')).toEqual({ type: 'clear' });
  });

  it('returns clear action for /reset', () => {
    expect(parseSlashCommand('/reset')).toEqual({ type: 'clear' });
  });

  it('returns clear action for /CLEAR (case insensitive)', () => {
    expect(parseSlashCommand('/CLEAR')).toEqual({ type: 'clear' });
  });

  it('returns session action for /session', () => {
    expect(parseSlashCommand('/session')).toEqual({ type: 'session' });
  });

  it('returns bridge-forward for unknown slash commands', () => {
    expect(parseSlashCommand('/provider gemini')).toEqual({ type: 'bridge-forward', text: '/provider gemini' });
    expect(parseSlashCommand('/search hello')).toEqual({ type: 'bridge-forward', text: '/search hello' });
    expect(parseSlashCommand('/today')).toEqual({ type: 'bridge-forward', text: '/today' });
    expect(parseSlashCommand('/unknown')).toEqual({ type: 'bridge-forward', text: '/unknown' });
  });

  it('handles trailing whitespace in /provider', () => {
    expect(parseSlashCommand('/provider  ')).toEqual({ type: 'provider' });
  });
});

describe('SLASH_COMMANDS', () => {
  it('contains at least provider, model, session, clear, reset', () => {
    const names = SLASH_COMMANDS.map((c) => c.command);
    expect(names).toContain('provider');
    expect(names).toContain('model');
    expect(names).toContain('session');
    expect(names).toContain('clear');
    expect(names).toContain('reset');
  });

  it('every entry has a non-empty command and description', () => {
    for (const cmd of SLASH_COMMANDS) {
      expect(cmd.command.length).toBeGreaterThan(0);
      expect(cmd.description.length).toBeGreaterThan(0);
    }
  });
});

describe('getSlashCompletions', () => {
  it('returns empty array for non-slash input', () => {
    expect(getSlashCompletions('hello')).toHaveLength(0);
    expect(getSlashCompletions('')).toHaveLength(0);
    expect(getSlashCompletions('p')).toHaveLength(0);
  });

  it('returns all commands for bare "/"', () => {
    const results = getSlashCompletions('/');
    expect(results.length).toBe(SLASH_COMMANDS.length);
  });

  it('returns matching commands for "/p"', () => {
    const results = getSlashCompletions('/p');
    expect(results.every((c) => c.command.startsWith('p'))).toBe(true);
    expect(results.map((c) => c.command)).toContain('provider');
  });

  it('returns only /model for "/mo"', () => {
    const results = getSlashCompletions('/mo');
    expect(results.map((c) => c.command)).toContain('model');
    expect(results.every((c) => c.command.startsWith('mo'))).toBe(true);
  });

  it('returns empty for fully typed command with trailing space', () => {
    // Once user typed "/provider " there's no need to show completions
    expect(getSlashCompletions('/provider ')).toHaveLength(0);
    expect(getSlashCompletions('/clear something')).toHaveLength(0);
  });

  it('is case insensitive', () => {
    const lower = getSlashCompletions('/p');
    const upper = getSlashCompletions('/P');
    expect(upper.map((c) => c.command)).toEqual(lower.map((c) => c.command));
  });

  it('returns empty for unmatched prefix', () => {
    expect(getSlashCompletions('/zzzunknown')).toHaveLength(0);
  });

  it('returns SlashCommandDef objects with command and description', () => {
    const results = getSlashCompletions('/');
    for (const r of results) {
      expect(typeof r.command).toBe('string');
      expect(typeof r.description).toBe('string');
    }
  });
});

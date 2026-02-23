import { describe, it, expect } from 'vitest';
import { parseSlashCommand } from '../slashCommands.js';

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

  it('returns bridge-forward for unknown slash commands', () => {
    expect(parseSlashCommand('/tool gemini')).toEqual({ type: 'bridge-forward', text: '/tool gemini' });
    expect(parseSlashCommand('/search hello')).toEqual({ type: 'bridge-forward', text: '/search hello' });
    expect(parseSlashCommand('/today')).toEqual({ type: 'bridge-forward', text: '/today' });
    expect(parseSlashCommand('/unknown')).toEqual({ type: 'bridge-forward', text: '/unknown' });
  });

  it('handles trailing whitespace in /provider', () => {
    expect(parseSlashCommand('/provider  ')).toEqual({ type: 'provider' });
  });
});

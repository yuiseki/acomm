import { describe, expect, it } from 'vitest';
import { buildInitialProviderSyncCommand } from '../startupProviderSync.js';

describe('buildInitialProviderSyncCommand', () => {
  it('returns null when desired and current match', () => {
    expect(buildInitialProviderSyncCommand('Gemini', 'Gemini')).toBeNull();
    expect(buildInitialProviderSyncCommand('dummy', 'Dummy')).toBeNull();
  });

  it('returns a provider slash command when bridge differs', () => {
    expect(buildInitialProviderSyncCommand('Dummy', 'Gemini')).toBe('/provider dummy');
    expect(buildInitialProviderSyncCommand('claude', 'Gemini')).toBe('/provider claude');
  });

  it('normalizes dummy aliases', () => {
    expect(buildInitialProviderSyncCommand('dummy-bot', 'Gemini')).toBe('/provider dummy');
    expect(buildInitialProviderSyncCommand('dummybot', 'Gemini')).toBe('/provider dummy');
  });
});

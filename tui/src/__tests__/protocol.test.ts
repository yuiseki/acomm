import { describe, it, expect } from 'vitest';
import { eventKind, providerCommandName, AGENT_PROVIDERS, PROVIDER_MODELS, normalizeProvider, getModelsForProvider } from '../protocol.js';
import type { ProtocolEvent, AgentProvider } from '../protocol.js';

describe('eventKind', () => {
  it('returns "Prompt" for a Prompt event', () => {
    const e: ProtocolEvent = { Prompt: { text: 'hello', provider: null, channel: null } };
    expect(eventKind(e)).toBe('Prompt');
  });

  it('returns "AgentChunk" for an AgentChunk event', () => {
    const e: ProtocolEvent = { AgentChunk: { chunk: 'hi', channel: null } };
    expect(eventKind(e)).toBe('AgentChunk');
  });

  it('returns "AgentDone" for an AgentDone event', () => {
    const e: ProtocolEvent = { AgentDone: { channel: null } };
    expect(eventKind(e)).toBe('AgentDone');
  });

  it('returns "ProviderSwitched" for a ProviderSwitched event', () => {
    const e: ProtocolEvent = { ProviderSwitched: { provider: 'Claude' } };
    expect(eventKind(e)).toBe('ProviderSwitched');
  });

  it('returns "SystemMessage" for a SystemMessage event', () => {
    const e: ProtocolEvent = { SystemMessage: { msg: 'ok', channel: null } };
    expect(eventKind(e)).toBe('SystemMessage');
  });

  it('returns "StatusUpdate" for a StatusUpdate event', () => {
    const e: ProtocolEvent = { StatusUpdate: { is_processing: true, channel: null } };
    expect(eventKind(e)).toBe('StatusUpdate');
  });

  it('returns "SyncContext" for a SyncContext event', () => {
    const e: ProtocolEvent = { SyncContext: { context: 'ctx' } };
    expect(eventKind(e)).toBe('SyncContext');
  });
});

describe('providerCommandName', () => {
  it('lowercases Gemini', () => expect(providerCommandName('Gemini')).toBe('gemini'));
  it('lowercases Claude', () => expect(providerCommandName('Claude')).toBe('claude'));
  it('lowercases Codex', () => expect(providerCommandName('Codex')).toBe('codex'));
  it('lowercases OpenCode', () => expect(providerCommandName('OpenCode')).toBe('opencode'));
  it('lowercases Dummy', () => expect(providerCommandName('Dummy')).toBe('dummy'));
  it('lowercases Mock', () => expect(providerCommandName('Mock')).toBe('mock'));
});

describe('AGENT_PROVIDERS', () => {
  it('contains exactly 5 providers', () => expect(AGENT_PROVIDERS).toHaveLength(5));
  it('includes Dummy (user-facing echo provider)', () => expect(AGENT_PROVIDERS).toContain('Dummy'));
  it('does not include Mock (internal only)', () => expect(AGENT_PROVIDERS).not.toContain('Mock'));
  it('starts with OpenCode', () => expect(AGENT_PROVIDERS[0]).toBe('OpenCode'));
});

describe('normalizeProvider', () => {
  it('falls back to OpenCode when missing or invalid', () => {
    expect(normalizeProvider(undefined)).toBe('OpenCode');
    expect(normalizeProvider('unknown')).toBe('OpenCode');
  });

  it('normalizes dummy aliases', () => {
    expect(normalizeProvider('dummy')).toBe('Dummy');
    expect(normalizeProvider('dummy-bot')).toBe('Dummy');
    expect(normalizeProvider('dummybot')).toBe('Dummy');
  });
});

describe('PROVIDER_MODELS', () => {
  it('has an entry for every AGENT_provider', () => {
    for (const provider of AGENT_PROVIDERS) {
      expect(PROVIDER_MODELS).toHaveProperty(provider);
    }
  });

  it('each entry is a non-empty array of strings', () => {
    for (const provider of AGENT_PROVIDERS) {
      const models = PROVIDER_MODELS[provider as AgentProvider];
      expect(Array.isArray(models)).toBe(true);
      expect(models.length).toBeGreaterThan(0);
      for (const m of models) {
        expect(typeof m).toBe('string');
        expect(m.length).toBeGreaterThan(0);
      }
    }
  });

  it('Gemini has at least one model', () => {
    expect(PROVIDER_MODELS['Gemini'].length).toBeGreaterThanOrEqual(1);
  });

  it('Gemini list matches current Gemini CLI names (no stale gemini-2.0-flash)', () => {
    expect(PROVIDER_MODELS['Gemini']).toContain('gemini-2.5-pro');
    expect(PROVIDER_MODELS['Gemini']).toContain('gemini-2.5-flash');
    expect(PROVIDER_MODELS['Gemini']).toContain('gemini-2.5-flash-lite');
    expect(PROVIDER_MODELS['Gemini']).toContain('auto-gemini-2.5');
    expect(PROVIDER_MODELS['Gemini']).toContain('auto-gemini-3');
    expect(PROVIDER_MODELS['Gemini']).toContain('gemini-3-pro-preview');
    expect(PROVIDER_MODELS['Gemini']).toContain('gemini-3.1-pro-preview');
    expect(PROVIDER_MODELS['Gemini']).toContain('gemini-3.1-pro-preview-customtools');
    expect(PROVIDER_MODELS['Gemini']).not.toContain('gemini-2.0-flash');
  });

  it('Claude has at least one model', () => {
    expect(PROVIDER_MODELS['Claude'].length).toBeGreaterThanOrEqual(1);
  });

  it('Codex has at least one model', () => {
    expect(PROVIDER_MODELS['Codex'].length).toBeGreaterThanOrEqual(1);
  });

  it('OpenCode has at least one model', () => {
    expect(PROVIDER_MODELS['OpenCode'].length).toBeGreaterThanOrEqual(1);
  });

  it('OpenCode defaults to qwen3-coder:30b', () => {
    expect(PROVIDER_MODELS['OpenCode'][0]).toBe('qwen3-coder:30b');
    expect(getModelsForProvider('bogus')[0]).toBe('qwen3-coder:30b');
  });

  it('Dummy has at least one model', () => {
    expect(PROVIDER_MODELS['Dummy'].length).toBeGreaterThanOrEqual(1);
  });
});

describe('ModelSwitched event', () => {
  it('is recognized by eventKind', () => {
    const e: ProtocolEvent = { ModelSwitched: { model: 'gemini-2.5-flash' } };
    expect(eventKind(e)).toBe('ModelSwitched');
  });
});

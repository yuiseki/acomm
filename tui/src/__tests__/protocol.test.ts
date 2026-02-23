import { describe, it, expect } from 'vitest';
import { eventKind, toolCommandName, AGENT_TOOLS, PROVIDER_MODELS } from '../protocol.js';
import type { ProtocolEvent, AgentTool } from '../protocol.js';

describe('eventKind', () => {
  it('returns "Prompt" for a Prompt event', () => {
    const e: ProtocolEvent = { Prompt: { text: 'hello', tool: null, channel: null } };
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

  it('returns "ToolSwitched" for a ToolSwitched event', () => {
    const e: ProtocolEvent = { ToolSwitched: { tool: 'Claude' } };
    expect(eventKind(e)).toBe('ToolSwitched');
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

describe('toolCommandName', () => {
  it('lowercases Gemini', () => expect(toolCommandName('Gemini')).toBe('gemini'));
  it('lowercases Claude', () => expect(toolCommandName('Claude')).toBe('claude'));
  it('lowercases Codex', () => expect(toolCommandName('Codex')).toBe('codex'));
  it('lowercases OpenCode', () => expect(toolCommandName('OpenCode')).toBe('opencode'));
  it('lowercases Mock', () => expect(toolCommandName('Mock')).toBe('mock'));
});

describe('AGENT_TOOLS', () => {
  it('contains exactly 4 tools', () => expect(AGENT_TOOLS).toHaveLength(4));
  it('does not include Mock (internal only)', () => expect(AGENT_TOOLS).not.toContain('Mock'));
  it('starts with Gemini', () => expect(AGENT_TOOLS[0]).toBe('Gemini'));
});

describe('PROVIDER_MODELS', () => {
  it('has an entry for every AGENT_TOOL', () => {
    for (const tool of AGENT_TOOLS) {
      expect(PROVIDER_MODELS).toHaveProperty(tool);
    }
  });

  it('each entry is a non-empty array of strings', () => {
    for (const tool of AGENT_TOOLS) {
      const models = PROVIDER_MODELS[tool as AgentTool];
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

  it('Claude has at least one model', () => {
    expect(PROVIDER_MODELS['Claude'].length).toBeGreaterThanOrEqual(1);
  });

  it('Codex has at least one model', () => {
    expect(PROVIDER_MODELS['Codex'].length).toBeGreaterThanOrEqual(1);
  });

  it('OpenCode has at least one model', () => {
    expect(PROVIDER_MODELS['OpenCode'].length).toBeGreaterThanOrEqual(1);
  });
});

describe('ModelSwitched event', () => {
  it('is recognized by eventKind', () => {
    const e: ProtocolEvent = { ModelSwitched: { model: 'gemini-2.5-flash' } };
    expect(eventKind(e)).toBe('ModelSwitched');
  });
});

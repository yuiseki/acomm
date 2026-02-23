/**
 * Tests for bridge.ts — mocks net.createConnection so no real socket is needed.
 */
import { describe, it, expect, vi, type Mock } from 'vitest';
import { EventEmitter } from 'node:events';

// ---------- minimal socket / readline mocks ----------

class MockSocket extends EventEmitter {
  writable = true;
  write = vi.fn();
  destroy = vi.fn();
}

class MockReadline extends EventEmitter {
  close = vi.fn();
}

let mockSocket: MockSocket;
let mockRl: MockReadline;

vi.mock('node:net', () => ({
  default: {
    createConnection: vi.fn(() => {
      mockSocket = new MockSocket();
      return mockSocket;
    }),
  },
}));

vi.mock('node:readline', () => ({
  default: {
    createInterface: vi.fn(() => {
      mockRl = new MockReadline();
      return mockRl;
    }),
  },
}));

// Import after mocks are registered
const { connectBridge } = await import('../bridge.js');

// ---------- helpers ----------

function simulateLine(line: string) {
  mockRl.emit('line', line);
}

// ---------- tests ----------

describe('connectBridge', () => {
  it('calls onEvent for each valid JSON line', () => {
    const onEvent = vi.fn();
    const onClose = vi.fn();
    connectBridge(onEvent, onClose);

    simulateLine(JSON.stringify({ AgentDone: { channel: null } }));
    simulateLine(JSON.stringify({ SystemMessage: { msg: 'hi', channel: null } }));

    expect(onEvent).toHaveBeenCalledTimes(2);
    expect(onEvent.mock.calls[0]![0]).toEqual({ AgentDone: { channel: null } });
    expect(onEvent.mock.calls[1]![0]).toEqual({ SystemMessage: { msg: 'hi', channel: null } });
  });

  it('ignores blank lines', () => {
    const onEvent = vi.fn();
    connectBridge(onEvent, vi.fn());

    simulateLine('');
    simulateLine('   ');

    expect(onEvent).not.toHaveBeenCalled();
  });

  it('ignores malformed JSON without throwing', () => {
    const onEvent = vi.fn();
    connectBridge(onEvent, vi.fn());

    expect(() => simulateLine('not json {')).not.toThrow();
    expect(onEvent).not.toHaveBeenCalled();
  });

  it('calls onClose when the socket emits "close"', () => {
    const onClose = vi.fn();
    connectBridge(vi.fn(), onClose);

    mockSocket.emit('close');

    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('calls onClose on socket error', () => {
    const onClose = vi.fn();
    connectBridge(vi.fn(), onClose);

    mockSocket.emit('error', new Error('ECONNREFUSED'));

    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('send() writes JSON + newline to the socket', () => {
    const bridge = connectBridge(vi.fn(), vi.fn());
    const event = { Prompt: { text: 'hello', tool: null, channel: 'tui' } } as const;

    bridge.send(event as any);

    expect(mockSocket.write).toHaveBeenCalledWith(JSON.stringify(event) + '\n');
  });

  it('send() does nothing when socket is not writable', () => {
    const bridge = connectBridge(vi.fn(), vi.fn());
    mockSocket.writable = false;

    bridge.send({ AgentDone: { channel: null } });

    expect(mockSocket.write).not.toHaveBeenCalled();
  });

  it('close() destroys the socket and closes readline', () => {
    const bridge = connectBridge(vi.fn(), vi.fn());

    bridge.close();

    expect(mockRl.close).toHaveBeenCalled();
    expect(mockSocket.destroy).toHaveBeenCalled();
  });
});

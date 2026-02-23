/**
 * Bridge connection management for the acomm Unix domain socket.
 * Connects to /tmp/acomm.sock and reads/writes ProtocolEvents as JSONL.
 */

import net from 'node:net';
import readline from 'node:readline';
import { existsSync, unlinkSync } from 'node:fs';
import { spawn } from 'node:child_process';
import type { ProtocolEvent } from './protocol.js';

export const SOCKET_PATH = '/tmp/acomm.sock';

/** Try to connect to the socket and immediately close it. Resolves if alive, rejects if not. */
function testConnect(socketPath: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const sock = net.createConnection(socketPath);
    sock.on('connect', () => { sock.destroy(); resolve(); });
    sock.on('error', (err) => { sock.destroy(); reject(err); });
  });
}

/**
 * Ensures the bridge is running, spawning `acomm --bridge` in the background
 * if needed. Handles stale socket files from previous bridge runs.
 * Waits up to timeoutMs for the bridge to be ready before throwing.
 */
export async function ensureBridge(timeoutMs = 5000): Promise<void> {
  // If socket file exists, verify it's actually accepting connections.
  if (existsSync(SOCKET_PATH)) {
    try {
      await testConnect(SOCKET_PATH);
      return; // Bridge is alive.
    } catch {
      // Stale socket — remove it before starting a new bridge.
      try { unlinkSync(SOCKET_PATH); } catch { /* ignore */ }
    }
  }

  const proc = spawn('acomm', ['--bridge'], {
    detached: true,
    stdio: 'ignore',
  });
  proc.unref();

  const deadline = Date.now() + timeoutMs;
  while (true) {
    await new Promise<void>((r) => setTimeout(r, 100));
    if (Date.now() > deadline) {
      throw new Error(`Bridge did not start within ${timeoutMs}ms (${SOCKET_PATH})`);
    }
    if (existsSync(SOCKET_PATH)) {
      try {
        await testConnect(SOCKET_PATH);
        return; // Bridge is ready.
      } catch {
        // Not ready yet — keep waiting.
      }
    }
  }
}

export interface Bridge {
  /** Send a ProtocolEvent to the bridge. */
  send(event: ProtocolEvent): void;
  /** Close the socket connection. */
  close(): void;
}

/**
 * Connects to the bridge socket and returns a Bridge handle.
 * onEvent is called for each received ProtocolEvent.
 * onClose is called when the connection is lost.
 */
export function connectBridge(
  onEvent: (event: ProtocolEvent) => void,
  onClose: () => void,
): Bridge {
  const socket = net.createConnection(SOCKET_PATH);
  const rl = readline.createInterface({ input: socket, crlfDelay: Infinity });

  rl.on('line', (line) => {
    const trimmed = line.trim();
    if (!trimmed) return;
    try {
      const event = JSON.parse(trimmed) as ProtocolEvent;
      onEvent(event);
    } catch {
      // Ignore malformed JSON lines
    }
  });

  // readline re-emits socket errors to the Interface; add a handler to prevent
  // unhandled 'error' event crashes. The socket error handler below handles teardown.
  rl.on('error', () => { /* swallow – handled via socket 'error' */ });

  socket.on('close', onClose);
  socket.on('error', (_err) => {
    // Swallow errors to prevent unhandled rejection; onClose handles teardown
    onClose();
  });

  return {
    send(event: ProtocolEvent): void {
      if (socket.writable) {
        socket.write(JSON.stringify(event) + '\n');
      }
    },
    close(): void {
      rl.close();
      socket.destroy();
    },
  };
}

/**
 * Bridge connection management for the acomm Unix domain socket.
 * Connects to /tmp/acomm.sock and reads/writes ProtocolEvents as JSONL.
 */

import net from 'node:net';
import readline from 'node:readline';
import { existsSync } from 'node:fs';
import { spawn } from 'node:child_process';
import type { ProtocolEvent } from './protocol.js';

export const SOCKET_PATH = '/tmp/acomm.sock';

/**
 * Ensures the bridge is running, spawning `acomm --bridge` in the background
 * if the socket file does not yet exist. Waits up to timeoutMs for the socket
 * to appear before throwing.
 */
export async function ensureBridge(timeoutMs = 3000): Promise<void> {
  if (!existsSync(SOCKET_PATH)) {
    const proc = spawn('acomm', ['--bridge'], {
      detached: true,
      stdio: 'ignore',
    });
    proc.unref();
  }

  const deadline = Date.now() + timeoutMs;
  while (!existsSync(SOCKET_PATH)) {
    if (Date.now() > deadline) {
      throw new Error(`Bridge did not start within ${timeoutMs}ms (${SOCKET_PATH})`);
    }
    await new Promise<void>((r) => setTimeout(r, 100));
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

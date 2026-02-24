/**
 * Entry point for acomm-tui.
 *
 * Usage:
 *   npx tsx src/index.tsx [--channel <name>] [--provider <gemini|claude|codex|opencode>]
 *
 * Starts the acomm bridge if it is not already running, connects to the
 * Unix domain socket, then renders the Ink TUI.
 */

import React from 'react';
import { render } from 'ink';
import { parseArgs } from 'node:util';
import { ensureBridge, connectBridge } from './bridge.js';
import type { ProtocolEvent, AgentProvider } from './protocol.js';
import { DEFAULT_PROVIDER, normalizeProvider } from './protocol.js';
import App from './App.js';

// ---------- CLI argument parsing ----------

const { values } = parseArgs({
  options: {
    channel: { type: 'string', short: 'c', default: 'tui' },
    provider: { type: 'string', short: 't', default: DEFAULT_PROVIDER },
  },
  allowPositionals: false,
  strict: false,
});

const channel = values.channel as string;
const initialProvider = normalizeProvider(values.provider as string);

// ---------- Bootstrap ----------

async function main(): Promise<void> {
  // Ensure bridge is running (spawns acomm --bridge if socket is absent)
  try {
    await ensureBridge();
  } catch (err) {
    console.error((err as Error).message);
    process.exit(1);
  }

  // Subscriber set: App registers its handler here after mounting.
  // Using a Set allows multiple listeners without closing over stale refs.
  const subscribers = new Set<(e: ProtocolEvent) => void>();

  const subscribe = (cb: (e: ProtocolEvent) => void): void => { subscribers.add(cb); };
  const unsubscribe = (cb: (e: ProtocolEvent) => void): void => { subscribers.delete(cb); };

  const bridge = connectBridge(
    (event) => subscribers.forEach((cb) => cb(event)),
    () => process.exit(0),
  );

  const { waitUntilExit } = render(
    <App
      bridge={bridge}
      channel={channel}
      initialProvider={initialProvider}
      subscribe={subscribe}
      unsubscribe={unsubscribe}
    />,
    { exitOnCtrlC: true },
  );

  await waitUntilExit();
  bridge.close();
}

main().catch((err: unknown) => {
  console.error(err);
  process.exit(1);
});

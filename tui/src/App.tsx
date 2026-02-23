/**
 * Main TUI application component.
 *
 * Manages:
 *   - Bridge event subscription (AgentChunk, AgentDone, etc.)
 *   - Message history rendering
 *   - Multiline input state and submit flow
 *   - Tool switching (1-4 keys in normal mode)
 *   - Input history (persisted to ~/.cache/acomm/history.txt)
 */

import React, { useState, useEffect, useCallback, useRef } from 'react';
import { Box, Text, useApp, useInput } from 'ink';
import chalk from 'chalk';
import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
import type { Bridge } from './bridge.js';
import type { AgentTool, ProtocolEvent } from './protocol.js';
import { toolCommandName, AGENT_TOOLS } from './protocol.js';
import MultilineInput from './MultilineInput.js';

// ---------- history helpers ----------

const HISTORY_PATH = join(homedir(), '.cache', 'acomm', 'history.txt');

function loadHistory(): string[] {
  try {
    if (!existsSync(HISTORY_PATH)) return [];
    return readFileSync(HISTORY_PATH, 'utf8').split('\n').filter(Boolean);
  } catch {
    return [];
  }
}

function saveHistory(entries: string[]): void {
  try {
    mkdirSync(join(homedir(), '.cache', 'acomm'), { recursive: true });
    writeFileSync(HISTORY_PATH, entries.join('\n'));
  } catch {
    // non-fatal
  }
}

// ---------- message model ----------

interface Message {
  id: number;
  text: string; // may contain ANSI sequences from chalk
}

// ---------- constants ----------

const SPINNER = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

// ---------- App ----------

interface AppProps {
  bridge: Bridge;
  channel: string;
  initialTool?: AgentTool;
  subscribe: (cb: (e: ProtocolEvent) => void) => void;
  unsubscribe: (cb: (e: ProtocolEvent) => void) => void;
}

export default function App({ bridge, channel, initialTool = 'Gemini', subscribe, unsubscribe }: AppProps): React.JSX.Element {
  const { exit } = useApp();

  // --- message list ---
  const [messages, setMessages] = useState<Message[]>([]);
  const msgIdRef = useRef(0);
  const nextId = () => ++msgIdRef.current;

  const push = useCallback((text: string) => {
    setMessages((prev) => [...prev, { id: nextId(), text }]);
  }, []);

  // Append to the last message in-place (for streaming chunks)
  const appendToLast = useCallback((chunk: string) => {
    setMessages((prev) => {
      if (prev.length === 0) return prev;
      const last = prev[prev.length - 1]!;
      return [...prev.slice(0, -1), { ...last, text: last.text + chunk }];
    });
  }, []);

  // --- input state ---
  const [inputValue, setInputValue] = useState('');
  const [cursorOffset, setCursorOffset] = useState(0);

  // --- history ---
  const [history, setHistory] = useState<string[]>(loadHistory);
  const [historyIdx, setHistoryIdx] = useState<number | null>(null);
  const savedInputRef = useRef(''); // save current input before history navigation

  // --- tool / processing state ---
  const [activeTool, setActiveTool] = useState<AgentTool>(initialTool);
  const [isProcessing, setIsProcessing] = useState(false);

  const [spinnerIdx, setSpinnerIdx] = useState(0);

  useEffect(() => {
    if (!isProcessing) return;
    const id = setInterval(() => setSpinnerIdx((i) => (i + 1) % SPINNER.length), 100);
    return () => clearInterval(id);
  }, [isProcessing]);

  // --- bridge event handler ---
  // Wrapped in useCallback so its identity is stable across renders;
  // the subscribe/unsubscribe effect below only fires when deps actually change.
  const handleEvent = useCallback((event: ProtocolEvent) => {
    if ('Prompt' in event) {
      const { text, channel: ch } = event.Prompt;
      // Skip echoes of our own TUI prompts — handleSubmit already shows them locally.
      // Show prompts from other channels (ntfy, slack, etc.) so they're visible.
      if (ch === channel) return;
      push(chalk.bold(`\n[${ch ?? 'unknown'}] `) + text + '\n');
    } else if ('AgentChunk' in event) {
      const { chunk } = event.AgentChunk;
      if (chunk) appendToLast(chunk);
    } else if ('AgentDone' in event) {
      setIsProcessing(false);
      push('\n' + chalk.dim('--- (Done) ---') + '\n');
    } else if ('SystemMessage' in event) {
      push(chalk.yellow(`[System] ${event.SystemMessage.msg}`) + '\n');
    } else if ('StatusUpdate' in event) {
      setIsProcessing(event.StatusUpdate.is_processing);
    } else if ('ToolSwitched' in event) {
      setActiveTool(event.ToolSwitched.tool);
      push(chalk.cyan(`\n[Tool switched → ${event.ToolSwitched.tool}]\n`));
    } else if ('SyncContext' in event) {
      push(chalk.dim('\n--- Today\'s Context ---\n') + event.SyncContext.context + chalk.dim('\n-----------------------\n'));
    }
  }, [push, appendToLast, channel]);

  // Register with the subscriber set provided by index.tsx.
  // The cleanup function automatically deregisters on unmount or when deps change.
  useEffect(() => {
    subscribe(handleEvent);
    return () => unsubscribe(handleEvent);
  }, [handleEvent, subscribe, unsubscribe]);

  // --- submit ---
  const handleSubmit = useCallback(
    (text: string) => {
      const trimmed = text.trim();
      if (!trimmed || isProcessing) return;

      // Save to history
      const next = history[history.length - 1] === trimmed ? history : [...history, trimmed];
      setHistory(next);
      saveHistory(next);
      setHistoryIdx(null);

      // Reset input
      setInputValue('');
      setCursorOffset(0);

      // Show in local message list immediately
      push(chalk.bold(`[you] `) + trimmed + '\n');
      setIsProcessing(true);

      // Prepare first agent line (chunks from AgentChunk will be appended here)
      push(chalk.green(`[${toolCommandName(activeTool)}] `));

      // Send to bridge
      bridge.send({ Prompt: { text: trimmed, tool: activeTool, channel } });
    },
    [history, isProcessing, channel, activeTool, bridge, push],
  );

  // --- history navigation ---
  const handleHistoryUp = useCallback(() => {
    if (history.length === 0) return;
    if (historyIdx === null) {
      savedInputRef.current = inputValue;
      const idx = history.length - 1;
      setHistoryIdx(idx);
      const val = history[idx]!;
      setInputValue(val);
      setCursorOffset(val.length);
    } else if (historyIdx > 0) {
      const idx = historyIdx - 1;
      setHistoryIdx(idx);
      const val = history[idx]!;
      setInputValue(val);
      setCursorOffset(val.length);
    }
  }, [history, historyIdx, inputValue]);

  const handleHistoryDown = useCallback(() => {
    if (historyIdx === null) return;
    if (historyIdx < history.length - 1) {
      const idx = historyIdx + 1;
      setHistoryIdx(idx);
      const val = history[idx]!;
      setInputValue(val);
      setCursorOffset(val.length);
    } else {
      setHistoryIdx(null);
      setInputValue(savedInputRef.current);
      setCursorOffset(savedInputRef.current.length);
    }
  }, [history, historyIdx]);

  // --- global keys (q to quit, 1-4 for tool switch) ---
  useInput((input, key) => {
    // q — quit (only when input is empty)
    if (input === 'q' && !key.ctrl && !key.shift && inputValue === '') {
      bridge.close();
      exit();
      return;
    }
    // 1-4 — switch tool
    const toolIdx = parseInt(input, 10) - 1;
    if (toolIdx >= 0 && toolIdx < AGENT_TOOLS.length && !key.ctrl && !key.shift && inputValue === '') {
      const tool = AGENT_TOOLS[toolIdx]!;
      setActiveTool(tool);
      bridge.send({ Prompt: { text: `/tool ${toolCommandName(tool)}`, tool: null, channel: null } });
    }
  });

  // --- render ---
  const statusLine = isProcessing
    ? chalk.yellow(`${SPINNER[spinnerIdx]} thinking...  [${toolCommandName(activeTool)}]`)
    : chalk.cyan(`[${toolCommandName(activeTool)}]  q=quit  1-4=switch tool`);

  return (
    <Box flexDirection="column" height="100%">
      {/* Status bar */}
      <Box borderStyle="single" borderColor="gray">
        <Text>{statusLine}</Text>
      </Box>

      {/* Message history */}
      <Box flexDirection="column" flexGrow={1} overflowY="hidden">
        {messages.map((m) => (
          <Text key={m.id}>{m.text}</Text>
        ))}
      </Box>

      {/* Multiline input */}
      <MultilineInput
        value={inputValue}
        cursorOffset={cursorOffset}
        isProcessing={isProcessing}
        activeTool={toolCommandName(activeTool)}
        onChangeCursor={setCursorOffset}
        onChangeValue={(v, c) => { setInputValue(v); setCursorOffset(c); }}
        onSubmit={handleSubmit}
        onHistoryUp={handleHistoryUp}
        onHistoryDown={handleHistoryDown}
      />
    </Box>
  );
}

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
import { toolCommandName, AGENT_TOOLS, PROVIDER_MODELS } from './protocol.js';
import MultilineInput from './MultilineInput.js';
import SelectionMenu from './SelectionMenu.js';
import SlashAutocomplete from './SlashAutocomplete.js';
import { parseSlashCommand, getSlashCompletions } from './slashCommands.js';
import { renderMarkdown } from './renderMarkdown.js';

type MenuMode = null | 'provider' | 'model';

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
  /** ANSI-colored prefix shown verbatim (e.g. chalk.green("[gemini] ")). */
  prefix: string;
  /** Message body — raw markdown for agent messages, plain/ANSI text for others. */
  text: string;
  /** True for agent response messages; enables markdown rendering when complete. */
  isAgent: boolean;
  /** True while AgentChunk events are still arriving; suppresses markdown rendering. */
  isStreaming: boolean;
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

  const push = useCallback((
    text: string,
    opts?: { prefix?: string; isAgent?: boolean; isStreaming?: boolean },
  ) => {
    setMessages((prev) => [
      ...prev,
      {
        id: nextId(),
        prefix: opts?.prefix ?? '',
        text,
        isAgent: opts?.isAgent ?? false,
        isStreaming: opts?.isStreaming ?? false,
      },
    ]);
  }, []);

  // Append to the last message in-place (for streaming chunks)
  const appendToLast = useCallback((chunk: string) => {
    setMessages((prev) => {
      if (prev.length === 0) return prev;
      const last = prev[prev.length - 1]!;
      return [...prev.slice(0, -1), { ...last, text: last.text + chunk }];
    });
  }, []);

  // Mark the last message as complete (stops streaming, enables markdown rendering)
  const markLastComplete = useCallback(() => {
    setMessages((prev) => {
      if (prev.length === 0) return prev;
      const last = prev[prev.length - 1]!;
      return [...prev.slice(0, -1), { ...last, isStreaming: false }];
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
  const [activeModel, setActiveModel] = useState<string>(PROVIDER_MODELS[initialTool][0] ?? '');
  const [isProcessing, setIsProcessing] = useState(false);

  // --- menu mode state ---
  const [menuMode, setMenuMode] = useState<MenuMode>(null);
  const [menuSelectedIndex, setMenuSelectedIndex] = useState(0);

  // --- slash command autocomplete ---
  // Completions are derived from inputValue on each render (no extra state needed).
  // autocompleteIdx tracks which completion is currently highlighted.
  const [autocompleteIdx, setAutocompleteIdx] = useState(0);
  // Dismissed state: user pressed Escape to hide the dropdown for the current input.
  const [autocompleteDismissed, setAutocompleteDismissed] = useState(false);

  // True from Prompt echo until the first AgentChunk arrives; drives the inline spinner.
  const [awaitingFirstChunk, setAwaitingFirstChunk] = useState(false);

  const [spinnerIdx, setSpinnerIdx] = useState(0);

  useEffect(() => {
    if (!isProcessing) return;
    const id = setInterval(() => setSpinnerIdx((i) => (i + 1) % SPINNER.length), 100);
    return () => clearInterval(id);
  }, [isProcessing]);

  // Compute slash completions from the current input (derived, not stored in state).
  const slashCompletions = autocompleteDismissed ? [] : getSlashCompletions(inputValue);

  // Reset autocomplete index when the completion list changes length.
  useEffect(() => {
    setAutocompleteIdx(0);
  }, [slashCompletions.length]);

  // Tab handler: insert the currently selected completion into the input.
  const handleTabComplete = useCallback(() => {
    if (slashCompletions.length === 0) return;
    const cmd = slashCompletions[autocompleteIdx]?.command ?? slashCompletions[0]?.command;
    if (!cmd) return;
    const newVal = `/${cmd} `;
    setInputValue(newVal);
    setCursorOffset(newVal.length);
    setAutocompleteDismissed(true);
  }, [slashCompletions, autocompleteIdx]);

  // --- bridge event handler ---
  // Wrapped in useCallback so its identity is stable across renders;
  // the subscribe/unsubscribe effect below only fires when deps actually change.
  const handleEvent = useCallback((event: ProtocolEvent) => {
    if ('Prompt' in event) {
      const { text, tool: eventTool } = event.Prompt;
      // Display ALL Prompt events (live echoes AND backlog replays).
      // handleSubmit no longer pushes locally; the bridge echo is the single source of truth.
      push(chalk.bold(`[you] `) + text + '\n');
      // Pre-push agent message placeholder: starts empty, chunks accumulate into `text`.
      const displayTool = toolCommandName(eventTool ?? activeTool);
      push('', {
        prefix: chalk.green(`[${displayTool}] `),
        isAgent: true,
        isStreaming: true,
      });
      setAwaitingFirstChunk(true);
    } else if ('AgentChunk' in event) {
      const { chunk } = event.AgentChunk;
      if (chunk) {
        setAwaitingFirstChunk(false);
        appendToLast(chunk);
      }
    } else if ('AgentDone' in event) {
      setAwaitingFirstChunk(false);
      setIsProcessing(false);
      // Mark the agent message complete so markdown rendering activates.
      markLastComplete();
      push('\n' + chalk.dim('--- (Done) ---') + '\n');
    } else if ('SystemMessage' in event) {
      push(chalk.yellow(`[System] ${event.SystemMessage.msg}`) + '\n');
    } else if ('StatusUpdate' in event) {
      setIsProcessing(event.StatusUpdate.is_processing);
    } else if ('ToolSwitched' in event) {
      const newTool = event.ToolSwitched.tool;
      setActiveTool(newTool);
      // Reset model to the first available model for the new tool
      setActiveModel(PROVIDER_MODELS[newTool][0] ?? '');
      push(chalk.cyan(`\n[Tool switched → ${newTool}]\n`));
    } else if ('ModelSwitched' in event) {
      setActiveModel(event.ModelSwitched.model);
      push(chalk.cyan(`\n[Model switched → ${event.ModelSwitched.model}]\n`));
    } else if ('SyncContext' in event) {
      // Suppress — context is injected into the agent prompt, not shown to the user.
    }
  }, [push, appendToLast, markLastComplete, activeTool]);

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

      // Parse slash commands before sending to bridge
      const action = parseSlashCommand(trimmed);
      if (action) {
        // Reset input immediately for all slash commands
        setInputValue('');
        setCursorOffset(0);

        if (action.type === 'provider') {
          setMenuSelectedIndex(0);
          setMenuMode('provider');
          return;
        }
        if (action.type === 'model') {
          setMenuSelectedIndex(0);
          setMenuMode('model');
          return;
        }
        if (action.type === 'clear') {
          // Clear local messages and reset bridge session
          setMessages([]);
          bridge.send({ Prompt: { text: '/clear', tool: null, channel: null } });
          return;
        }
        // bridge-forward: fall through to normal submission below (trimmed is action.text)
      }

      // Save to history
      const next = history[history.length - 1] === trimmed ? history : [...history, trimmed];
      setHistory(next);
      saveHistory(next);
      setHistoryIdx(null);

      // Reset input and autocomplete state
      setInputValue('');
      setCursorOffset(0);
      setAutocompleteDismissed(false);

      // Optimistically mark as processing; the bridge echo will display [you] + agent prefix.
      setIsProcessing(true);

      // Send to bridge — the echo triggers handleEvent which shows [you] msg + agent prefix.
      bridge.send({ Prompt: { text: trimmed, tool: activeTool, channel } });
    },
    [history, isProcessing, channel, activeTool, bridge],
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

  // --- global keys (q to quit, 1-4 for tool switch; menu navigation when in menu mode) ---
  useInput((input, key) => {
    // Menu mode: intercept all navigation and selection keys
    if (menuMode !== null) {
      if (key.upArrow) {
        setMenuSelectedIndex((i) => Math.max(0, i - 1));
        return;
      }
      if (key.downArrow) {
        const count = menuMode === 'provider' ? AGENT_TOOLS.length : PROVIDER_MODELS[activeTool].length;
        setMenuSelectedIndex((i) => Math.min(count - 1, i + 1));
        return;
      }
      if (key.return) {
        if (menuMode === 'provider') {
          const tool = AGENT_TOOLS[menuSelectedIndex]!;
          bridge.send({ Prompt: { text: `/tool ${toolCommandName(tool)}`, tool: null, channel: null } });
        } else if (menuMode === 'model') {
          const model = PROVIDER_MODELS[activeTool][menuSelectedIndex];
          if (model) {
            bridge.send({ Prompt: { text: `/model ${model}`, tool: null, channel: null } });
          }
        }
        setMenuMode(null);
        return;
      }
      if (key.escape || input === 'q') {
        setMenuMode(null);
        return;
      }
      return; // Swallow all other keys in menu mode
    }

    // Normal mode
    // Escape — dismiss slash autocomplete if showing
    if (key.escape && slashCompletions.length > 0) {
      setAutocompleteDismissed(true);
      return;
    }

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
  const modelLabel = activeModel ? `/${activeModel}` : '';
  const statusLine = chalk.cyan(
    `[${toolCommandName(activeTool)}${modelLabel}]  q=quit  1-4=switch tool  /provider  /model  /clear`,
  );

  // Menu items for current mode
  const menuItems =
    menuMode === 'provider'
      ? AGENT_TOOLS.map((t, i) => `${i + 1}. ${toolCommandName(t)}`)
      : menuMode === 'model'
      ? PROVIDER_MODELS[activeTool].map((m, i) => `${i + 1}. ${m}`)
      : [];

  const menuTitle =
    menuMode === 'provider' ? 'Select provider' : 'Select model';

  return (
    <Box flexDirection="column" height="100%">
      {/* Status bar */}
      <Box borderStyle="single" borderColor="gray">
        <Text>{statusLine}</Text>
      </Box>

      {/* Message history */}
      <Box flexDirection="column" flexGrow={1} overflowY="hidden">
        {messages.map((m, i) => {
          const isLast = i === messages.length - 1;
          // Render markdown for agent messages once streaming is complete.
          const body = m.isAgent && !m.isStreaming
            ? renderMarkdown(m.text)
            : m.text;
          const displayText = isLast && awaitingFirstChunk
            ? m.prefix + body + chalk.yellow(`${SPINNER[spinnerIdx]} thinking...`)
            : m.prefix + body;
          return <Text key={m.id}>{displayText}</Text>;
        })}
      </Box>

      {/* Selection menu overlay (shown instead of input during menu mode) */}
      {menuMode !== null ? (
        <SelectionMenu
          title={menuTitle}
          items={menuItems}
          selectedIndex={menuSelectedIndex}
        />
      ) : (
        <>
          {/* Slash command autocomplete dropdown (shown above input when typing '/...') */}
          <SlashAutocomplete
            completions={slashCompletions}
            selectedIndex={autocompleteIdx}
          />
          <MultilineInput
            value={inputValue}
            cursorOffset={cursorOffset}
            isProcessing={isProcessing}
            activeTool={toolCommandName(activeTool)}
            isActive={menuMode === null}
            hasCompletions={slashCompletions.length > 0}
            onChangeCursor={setCursorOffset}
            onChangeValue={(v, c) => { setInputValue(v); setCursorOffset(c); setAutocompleteDismissed(false); }}
            onSubmit={handleSubmit}
            onHistoryUp={handleHistoryUp}
            onHistoryDown={handleHistoryDown}
            onTabComplete={handleTabComplete}
          />
        </>
      )}
    </Box>
  );
}

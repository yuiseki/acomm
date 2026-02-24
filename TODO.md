# acomm TUI — TypeScript Migration Work Plan

## Background

The Rust TUI (`src/tui.rs`) had two unresolved bugs:

1. **`q` key does not exit cleanly** — root cause: `event::poll` / `event::read` are
   synchronous blocking calls inside `tokio::spawn`. `abort()` only fires at `.await`
   points, so the task hangs and the process must be killed.

2. **Shift+Enter newline not working** — root cause: crossterm's keyboard-enhancement
   push (`PushKeyboardEnhancementFlags`) only works in terminals that implement the
   Kitty keyboard protocol. Most terminals (including iTerm2 default mode) do not support
   it, so Shift+Enter is indistinguishable from plain Enter.

**Decision**: Keep the Rust bridge/pub-sub intact; replace the Rust TUI with a
TypeScript implementation using `@jrichman/ink` (the same Ink fork that gemini-cli uses),
which correctly exposes `key.shift` in the input handler.

---

## Current State

| File | Status |
|------|--------|
| `tui/src/protocol.ts` | ✅ Done — ProtocolEvent types, AgentProvider, helpers |
| `tui/src/bridge.ts` | ✅ Done — ensureBridge, connectBridge, Bridge interface |
| `tui/src/textHelpers.ts` | ✅ Done — pure text helpers (insertAt, deleteCharBefore, offsetToRowCol, rowColToOffset) |
| `tui/src/MultilineInput.tsx` | ✅ Done — imports textHelpers; Shift+Enter newline, cursor, history keys |
| `tui/src/App.tsx` | ✅ Done — subscribe/unsubscribe props; no _eventHandlerRef hack |
| `tui/src/index.tsx` | ✅ Done — subscriber Set pattern, renders `<App>` |
| `tui/package.json` | ✅ Done — `@jrichman/ink@6.4.11`, tsx, vitest; bin: acomm-tui |
| `tui/tsconfig.json` | ✅ Done |
| `tui/src/__tests__/protocol.test.ts` | ✅ Done — 15 tests passing |
| `tui/src/__tests__/multilineInput.test.ts` | ✅ Done — 19 tests passing |
| `tui/src/__tests__/bridge.test.ts` | ✅ Done — 8 tests passing (42 total) |
| `repos/yuiclaw/src/process.rs` | ✅ Done — launches acomm-tui (falls back to acomm) |
| `repos/yuiclaw/Makefile` | ✅ Done — install-acomm-tui target; `npm link` to install bin |
| Rust TUI (`src/tui.rs`) | ⚠️ Still in tree — deprecate / remove after TS TUI is validated in production |

---

## Task List (ordered by priority)

### ✅ P0 — Fix App.tsx: replace `_eventHandlerRef` hack with props  DONE

`subscribe`/`unsubscribe` props added; `handleEvent` wrapped in `useCallback`;
`useEffect` registers/deregisters cleanly on mount/unmount.

### ✅ P1 — Write unit tests (vitest)  DONE

42 tests across 3 files — all pass, zero errors.

### ✅ P2 — npm scripts  DONE

`package.json` already had: start, dev, typecheck, test; plus `bin: acomm-tui`.

### ✅ P3 — Update yuiclaw to launch the TypeScript TUI  DONE

`process.rs` `start_stack()` now checks for `acomm-tui` in PATH via `which`; if found,
execs `acomm-tui --tool <tool>`. Falls back to `acomm` for backwards compatibility.
All Japanese comments replaced with English.

### ✅ P4 — Update yuiclaw Makefile  DONE

`install-acomm-tui` target: `npm install --legacy-peer-deps` + `npm link`.
`install-deps` chain updated: `... install-acomm install-acomm-tui`.
`test` target now also runs `npx vitest run` in `deps/acomm/tui`.

---

### P5 — Deprecate / remove Rust TUI code  (remaining)

Once the TypeScript TUI is validated in production:

1. Remove or stub `start_tui()` in `repos/acomm/src/main.rs`
   (the `--tui` flag can print a deprecation notice or be dropped).
2. Delete `repos/acomm/src/tui.rs`.
3. Update `repos/acomm/README.md` (if exists) to document the new architecture.

---

## Architecture

```
┌──────────────────────────────────────────────────────────┐
│  Terminal                                                │
│  ┌────────────────────────────────────────────────────┐  │
│  │  acomm-tui  (TypeScript / Ink)                     │  │
│  │  ┌──────────┐  subscribe/unsubscribe  ┌──────────┐ │  │
│  │  │ index.tsx│ ──────────────────────▶ │ App.tsx  │ │  │
│  │  │          │ ◀── ProtocolEvent ────── │          │ │  │
│  │  └────┬─────┘                         └──────────┘ │  │
│  │       │ bridge.ts (net.Socket / JSONL)              │  │
│  └───────┼────────────────────────────────────────────┘  │
│          │ /tmp/acomm.sock                               │
│  ┌───────▼────────────────────────────────────────────┐  │
│  │  acomm --bridge  (Rust / tokio)                    │  │
│  │  pub/sub hub, agent runners (gemini/claude/etc.)   │  │
│  └────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────┘
```

---

## How to Run

```bash
# Start the Rust bridge
acomm --bridge &   # or: yuiclaw start (which also inits amem/abeat)

# Run the TypeScript TUI directly (dev mode)
cd /home/yuiseki/Workspaces/repos/acomm/tui
npx tsx src/index.tsx --tool gemini

# Or via installed bin (after make install-acomm-tui)
acomm-tui --tool claude

# Run all TUI tests
npx vitest run

# TypeScript type check
npx tsc --noEmit
```

## How to install globally

```bash
cd /home/yuiseki/Workspaces/repos/yuiclaw
make install          # installs all components including acomm-tui
yuiclaw start         # launches bridge + acomm-tui
```

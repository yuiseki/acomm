/**
 * Markdown → ANSI terminal string renderer.
 *
 * Uses `marked` with the `marked-terminal` renderer so that markdown markup
 * (headings, bold, italic, code blocks, lists, blockquotes) is converted into
 * ANSI escape sequences that Ink / chalk can display.
 *
 * This is used to post-process completed agent messages — during streaming
 * the raw text is shown as-is for responsiveness; once AgentDone fires the
 * accumulated text is re-rendered as formatted markdown.
 */

import { marked, type MarkedExtension } from 'marked';
import { markedTerminal } from 'marked-terminal';

// Configure marked with the terminal renderer once at module load.
// Cast required: @types/marked-terminal returns TerminalRenderer (a Renderer subclass)
// but marked.use() expects MarkedExtension — the cast is safe at runtime.
marked.use(markedTerminal() as unknown as MarkedExtension);

/**
 * Convert a markdown string to an ANSI-formatted terminal string.
 *
 * Returns the input unchanged if it is empty or whitespace-only, so callers
 * don't have to special-case those situations.
 */
export function renderMarkdown(text: string): string {
  if (!text.trim()) return text;
  const result = marked(text);
  // marked() is synchronous when no async extensions are registered.
  // Cast to string to satisfy the TypeScript union type.
  return result as string;
}

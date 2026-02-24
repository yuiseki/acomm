/**
 * Markdown → ANSI terminal string renderer.
 *
 * Uses `marked` with the `marked-terminal` renderer so that markdown markup
 * (headings, bold, italic, lists, blockquotes) is converted into ANSI escape
 * sequences that Ink / chalk can display.
 *
 * Code blocks receive syntax highlighting via `cli-highlight`, which supports
 * 180+ languages (everything highlight.js supports).  Language is inferred
 * from the fenced code-block info string (e.g. ```typescript).
 *
 * This is used to post-process completed agent messages — during streaming
 * the raw text is shown as-is for responsiveness; once AgentDone fires the
 * accumulated text is re-rendered as formatted markdown.
 */

import { marked, type MarkedExtension } from 'marked';
import { markedTerminal } from 'marked-terminal';
import { highlight, supportsLanguage } from 'cli-highlight';

// 1. Apply marked-terminal for general markdown → ANSI rendering
//    (headings, bold/italic, inline code, lists, blockquotes, tables…)
// Cast required: @types/marked-terminal returns TerminalRenderer (a Renderer
// subclass) but marked.use() expects MarkedExtension — safe at runtime.
marked.use(markedTerminal() as unknown as MarkedExtension);

// 2. Override fenced code blocks with cli-highlight for proper syntax colouring.
//    This runs after the terminal renderer and takes precedence for code tokens.
marked.use({
  renderer: {
    code(token): string {
      const { text, lang } = token;
      const language = lang && supportsLanguage(lang) ? lang : undefined;
      const highlighted = language
        ? highlight(text, { language, ignoreIllegals: true })
        : text;
      // Indent each line by 4 spaces to visually separate code from prose.
      const lines = highlighted.split('\n').map((l: string) => `    ${l}`);
      return lines.join('\n') + '\n\n';
    },
  },
} as MarkedExtension);

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
  return (result as string).replace(/\n+$/, '');
}

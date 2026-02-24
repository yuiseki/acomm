export interface InputKeyLike {
  return?: boolean;
  shift?: boolean;
  meta?: boolean;
  alt?: boolean;
  ctrl?: boolean;
}

/**
 * Classify "insert newline" inputs for MultilineInput.
 *
 * Important: A bare LF ("\n") without ctrl is not treated as newline insertion.
 * Some IME conversion-confirm flows can emit Enter-like events during commit, and
 * treating naked LF as Ctrl+J causes accidental line breaks inside Japanese text.
 */
export function shouldInsertNewline(input: string, key: InputKeyLike): boolean {
  return (
    (Boolean(key.return) && (Boolean(key.shift) || Boolean(key.meta) || Boolean(key.alt))) ||
    (input === '\n' && !key.return && Boolean(key.ctrl))
  );
}

/**
 * Normalize regular text input before insertion.
 *
 * IME commit on some terminals can include a trailing CR/LF in the same `input`
 * payload as the committed text (e.g. "長い\\n"). Strip only trailing CR/LF so:
 * - IME commits don't create accidental line breaks
 * - multiline paste with internal newlines still works
 */
export function normalizeInsertedInput(input: string): string {
  if (!input) return input;

  // Bare CR/LF should not be treated as regular text input.
  if (/^[\r\n]+$/.test(input)) return '';

  // Preserve multiline paste content; only strip trailing line endings.
  return input.replace(/[\r\n]+$/, '');
}

/**
 * Slash command autocomplete dropdown.
 *
 * Shown above the input field when the user's current input starts with '/'
 * and there are matching command completions.
 *
 * The selected entry is highlighted; press Tab in the input to insert it.
 * The first entry is pre-selected.
 */

import React from 'react';
import { Box, Text } from 'ink';
import chalk from 'chalk';
import type { SlashCommandDef } from './slashCommands.js';

interface Props {
  completions: SlashCommandDef[];
  selectedIndex: number;
}

export default function SlashAutocomplete({ completions, selectedIndex }: Props): React.JSX.Element | null {
  if (completions.length === 0) return null;

  return (
    <Box flexDirection="column" borderStyle="single" borderColor="cyan">
      {completions.map((cmd, i) => {
        const isSelected = i === selectedIndex;
        const cmdText = isSelected
          ? chalk.bgCyan(chalk.black(`/${cmd.command}`))
          : chalk.cyan(`/${cmd.command}`);
        const descText = chalk.dim(`  ${cmd.description}`);
        return (
          <Box key={cmd.command} paddingLeft={1}>
            <Text>{cmdText}{descText}</Text>
          </Box>
        );
      })}
      <Box paddingLeft={1}>
        <Text>{chalk.dim('Tab=complete  Esc=dismiss')}</Text>
      </Box>
    </Box>
  );
}

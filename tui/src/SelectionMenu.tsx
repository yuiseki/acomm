/**
 * Interactive selection menu component for Ink.
 *
 * Renders a bordered list of items with one item highlighted.
 * Navigation is controlled externally via selectedIndex.
 *
 * Key bindings (handled by parent via useInput):
 *   Up arrow    — move selection up
 *   Down arrow  — move selection down
 *   Enter       — confirm selection
 *   Escape / q  — cancel / close menu
 */

import React from 'react';
import { Box, Text } from 'ink';
import chalk from 'chalk';

interface SelectionMenuProps {
  title: string;
  items: string[];
  selectedIndex: number;
}

export default function SelectionMenu({
  title,
  items,
  selectedIndex,
}: SelectionMenuProps): React.JSX.Element {
  return (
    <Box flexDirection="column" borderStyle="round" borderColor="cyan" paddingX={1}>
      <Text>{chalk.cyan.bold(title)}</Text>
      <Text>{chalk.dim('↑/↓ to move  Enter to select  Esc to cancel')}</Text>
      {items.map((item, i) => {
        const isSelected = i === selectedIndex;
        const prefix = isSelected ? chalk.cyan('▶ ') : '  ';
        const label = isSelected ? chalk.cyan.bold(item) : item;
        return (
          <Text key={i}>{prefix}{label}</Text>
        );
      })}
    </Box>
  );
}

/**
 * shell-types.ts
 *
 * Registry of all shell types available in the universal terminal.
 *
 * How spawning works:
 *  The Rust backend spawns the user's login shell ($SHELL) as the host
 *  process, with the directories holding the distribution php and mysql
 *  clients prepended to PATH. `initialCommand` is a string written to the
 *  shell's stdin immediately after spawn - equivalent to the user typing it
 *  and pressing Enter. null means a plain shell prompt.
 *
 */

import {
  SquareTerminal,
  Code,
  Database,
  type LucideIcon,
} from "lucide-react";

export interface ShellType {
  id: "shell" | "bash" | "php-cli" | "mysql-cli";
  label: string;
  icon: LucideIcon;
  /**
   * Command written to the shell's stdin after spawn.
   * null = plain shell prompt (no initial command sent).
   */
  initialCommand: string | null;
}

export const SHELL_TYPES: ShellType[] = [
  {
    id: "shell",
    label: "Shell",
    icon: SquareTerminal,
    initialCommand: null,
  },
  {
    id: "bash",
    label: "Bash",
    icon: SquareTerminal,
    initialCommand: null, // plain login shell - identical to "shell" but semantically distinct
  },
  {
    id: "php-cli",
    label: "PHP Interactive",
    icon: Code,
    initialCommand: "php -a",
  },
  {
    id: "mysql-cli",
    label: "MySQL CLI",
    icon: Database,
    initialCommand: "mysql -u root",
  },

] as const;

/** Default shell opened when the terminal page first mounts. */
export const DEFAULT_SHELL = SHELL_TYPES[0];

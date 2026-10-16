// Equivalent `nfd` commands shown in Expert mode (PLAN §13 G2), so every action that
// the CLI can also do teaches the command for scripts and Task Scheduler.

/** Quote one argument for cmd.exe / PowerShell / the MSVC argv parser. */
export function cliArg(arg: string): string {
  if (arg && /^[\w:\\/.,+=@-]+$/.test(arg)) return arg;
  // backslashes right before the closing quote must be doubled ("C:\My Dir\\")
  return `"${arg.replace(/"/g, '\\"').replace(/(\\+)$/, "$1$1")}"`;
}

/** `nfd scan <root> …flags`. */
export function nfdScan(root: string, ...flags: string[]): string {
  return ["nfd", "scan", cliArg(root), ...flags].join(" ");
}

/** CLI size flag value: whole GiB as `2GB`, else MiB. */
export function sizeFlag(bytes: number): string {
  const gib = bytes / 2 ** 30;
  return Number.isInteger(gib) ? `${gib}GB` : `${Math.round(bytes / 2 ** 20)}MB`;
}

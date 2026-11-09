/**
 * "Chế độ nâng cao" — the switch that reveals the inherited cc-switch surface
 * (providers, MCP, prompts, proxy, usage…).
 *
 * Off by default: a WISA user is here to get Claude Code or Codex talking to the
 * gateway, and every extra control is a way to break that. The choice is
 * remembered per machine so the advanced user only flips it once.
 */

const KEY = "wisa-connect-advanced-mode";

export function readAdvancedMode(): boolean {
  try {
    return localStorage.getItem(KEY) === "1";
  } catch {
    return false;
  }
}

export function storeAdvancedMode(value: boolean): void {
  try {
    localStorage.setItem(KEY, value ? "1" : "0");
  } catch {
    // Storage unavailable: the toggle still works for this session.
  }
}

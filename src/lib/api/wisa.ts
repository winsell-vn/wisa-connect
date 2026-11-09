import { invoke } from "@tauri-apps/api/core";

/**
 * WISA account surface.
 *
 * Every call here lands in Rust (`src-tauri/src/wisa/`): the webview never sees
 * an access token or the account key, only the shapes below.
 */

export type WisaTargetId = "claude_code" | "codex_cli" | "claude_desktop_3p";

export interface WisaSessionView {
  signedIn: boolean;
  user: Record<string, unknown> | null;
  email: string | null;
}

export interface WisaKeyStatus {
  prefix: string;
  fingerprint: string;
  state: string;
  createdAt: string | null;
}

export interface WisaClientStatus {
  target: WisaTargetId;
  label: string;
  /** The client looks present on this machine. */
  installed: boolean;
  /** The `WISA Gateway` provider exists for this client *and* is the active one. */
  connected: boolean;
}

export interface WisaStatus {
  signedIn: boolean;
  email: string | null;
  key: WisaKeyStatus | null;
  clients: WisaClientStatus[];
}

export interface WisaProvisionOutcome {
  target: WisaTargetId;
  label: string;
  ok: boolean;
  /** Warnings or the failure reason; `null` on a clean success. */
  message: string | null;
}

export const wisaApi = {
  /** The current session, restored from the OS credential store on first call. */
  session: () => invoke<WisaSessionView>("wisa_session"),

  /** Opens the browser for Google sign-in and resolves once the session is stored. */
  login: () => invoke<WisaSessionView>("wisa_login"),

  /** Forgets the local session. The account key itself is untouched. */
  logout: () => invoke<void>("wisa_logout"),

  status: () => invoke<WisaStatus>("wisa_status"),

  /** Creates or refreshes the WISA Gateway provider and switches to it. */
  provision: (targets: WisaTargetId[]) =>
    invoke<WisaProvisionOutcome[]>("wisa_provision", { targets }),

  /** Removes the WISA provider from the given clients. */
  unlink: (targets: WisaTargetId[]) =>
    invoke<WisaProvisionOutcome[]>("wisa_unlink", { targets }),
};

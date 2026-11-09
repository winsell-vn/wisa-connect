import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { Loader2 } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";
import { wisaApi, type WisaSessionView } from "@/lib/api/wisa";
import { storeView } from "@/lib/navigation";
import { WisaMark } from "@/components/wisa/WisaMark";

/**
 * Sign-in gate for the whole window.
 *
 * The app is useless without a WISA account, so this sits in front of it rather
 * than being a page inside it: no sidebar, no providers, nothing to click until
 * there is a session. The session itself lives in Rust; this component only
 * ever sees the e-mail address.
 *
 * Copy is Vietnamese on purpose — the WISA-branded screens are Vietnamese-only
 * for now, unlike the inherited cc-switch surface behind "Chế độ nâng cao".
 */

const ACCOUNT_PAGE = "https://wisa.vn/account/api-connection";

interface WisaAuthValue {
  session: WisaSessionView;
  /** Re-reads the session. Call after signing out so the gate comes back. */
  refresh: () => Promise<void>;
}

const WisaAuthContext = createContext<WisaAuthValue | null>(null);

export function useWisaAuth(): WisaAuthValue {
  const value = useContext(WisaAuthContext);
  if (!value) {
    throw new Error("useWisaAuth phải nằm trong <WisaGate>");
  }
  return value;
}

function describeError(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Đã xảy ra lỗi không xác định.";
}

function GoogleMark({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 48 48" aria-hidden="true" className={className}>
      <path
        fill="#EA4335"
        d="M24 9.5c3.54 0 6.71 1.22 9.21 3.6l6.85-6.85C35.9 2.38 30.47 0 24 0 14.62 0 6.51 5.38 2.56 13.22l7.98 6.19C12.43 13.72 17.74 9.5 24 9.5Z"
      />
      <path
        fill="#4285F4"
        d="M46.98 24.55c0-1.57-.15-3.09-.38-4.55H24v9.02h12.94c-.58 2.96-2.26 5.48-4.78 7.18l7.73 6c4.51-4.18 7.09-10.36 7.09-17.65Z"
      />
      <path
        fill="#FBBC05"
        d="M10.53 28.59c-.48-1.45-.76-2.99-.76-4.59s.27-3.14.76-4.59l-7.98-6.19C.92 16.46 0 20.12 0 24c0 3.88.92 7.54 2.56 10.78l7.97-6.19Z"
      />
      <path
        fill="#34A853"
        d="M24 48c6.48 0 11.93-2.13 15.89-5.81l-7.73-6c-2.15 1.45-4.92 2.3-8.16 2.3-6.26 0-11.57-4.22-13.47-9.91l-7.98 6.19C6.51 42.62 14.62 48 24 48Z"
      />
    </svg>
  );
}

function SignInScreen({
  error,
  busy,
  onSignIn,
}: {
  error: string | null;
  busy: boolean;
  onSignIn: () => void;
}) {
  return (
    <div className="flex h-screen w-full flex-col items-center justify-center bg-app px-6 text-fg-1">
      <div className="w-full max-w-[420px]">
        <div className="mb-8 flex flex-col items-center text-center">
          <WisaMark className="h-14 w-14" />
          <h1 className="mt-5 text-xl font-semibold tracking-tight">
            WISA Connect
          </h1>
          <p className="mt-2 text-sm leading-relaxed text-fg-2">
            Đăng nhập để tự động cấu hình Claude Code, Codex và Claude Desktop
            bằng khoá WISA của bạn.
          </p>
        </div>

        <Button
          size="lg"
          className="h-12 w-full gap-3 bg-inverse-bg text-inverse-fg hover:bg-inverse-hover"
          onClick={onSignIn}
          disabled={busy}
        >
          {busy ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            <GoogleMark className="h-4 w-4" />
          )}
          {busy ? "Đang chờ trình duyệt…" : "Đăng nhập bằng Google"}
        </Button>

        {busy && (
          <p className="mt-3 text-center text-xs leading-relaxed text-fg-3">
            Hoàn tất đăng nhập trong cửa sổ trình duyệt vừa mở. Cửa sổ này tự
            tiếp tục khi xong.
          </p>
        )}

        {error && (
          <p
            role="alert"
            className="mt-3 rounded-lg bg-danger-soft px-3 py-2 text-xs leading-relaxed text-danger-text"
          >
            {error}
          </p>
        )}

        <div className="mt-10 space-y-2 border-t border-border-strong/40 pt-5 text-xs leading-relaxed text-fg-3">
          <p>
            Mật khẩu không đi qua ứng dụng này: bạn đăng nhập trực tiếp với
            Google, ứng dụng chỉ nhận mã truy cập một lần.
          </p>
          <p>
            Phiên đăng nhập được lưu trong Credential Manager của hệ điều hành,
            không nằm trong tệp cấu hình.
          </p>
          <button
            type="button"
            className="text-action-text underline-offset-2 hover:underline"
            onClick={() => void invoke("open_external", { url: ACCOUNT_PAGE })}
          >
            Xem khoá và mức sử dụng trên wisa.vn
          </button>
        </div>
      </div>
    </div>
  );
}

export function WisaGate({ children }: { children: ReactNode }) {
  const [session, setSession] = useState<WisaSessionView | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const next = await wisaApi.session();
      setSession(next);
      setError(null);
    } catch (cause) {
      // A failed restore is not a sign-out: the refresh token may simply be
      // gone. Show the sign-in screen with the reason instead of a dead app.
      setSession({ signedIn: false, user: null, email: null });
      setError(describeError(cause));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const signIn = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      const next = await wisaApi.login();
      setSession(next);
      // First landing after sign-in is the status page, not the provider list.
      storeView("wisa");
    } catch (cause) {
      setError(describeError(cause));
    } finally {
      setBusy(false);
    }
  }, []);

  const value = useMemo<WisaAuthValue | null>(
    () =>
      session?.signedIn ? { session, refresh: load } : null,
    [session, load],
  );

  if (loading) {
    return (
      <div className="flex h-screen w-full items-center justify-center bg-app">
        <Loader2 className="h-5 w-5 animate-spin text-fg-3" />
      </div>
    );
  }

  if (!value) {
    return (
      <SignInScreen error={error} busy={busy} onSignIn={() => void signIn()} />
    );
  }

  return (
    <WisaAuthContext.Provider value={value}>
      {children}
    </WisaAuthContext.Provider>
  );
}

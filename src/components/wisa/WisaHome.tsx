import { useCallback, useEffect, useState } from "react";
import {
  CheckCircle2,
  Circle,
  KeyRound,
  Loader2,
  LogOut,
  RefreshCw,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { AppPageHeader } from "@/components/shell/AppPageHeader";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { WisaMark } from "@/components/wisa/WisaMark";
import { useWisaAuth } from "@/components/wisa/WisaGate";
import { toast } from "@/lib/toast";
import {
  wisaApi,
  type WisaClientStatus,
  type WisaStatus,
  type WisaTargetId,
} from "@/lib/api/wisa";

/**
 * The landing page: what this account is, and whether each client is using it.
 *
 * One key, three clients. The page exists to answer a single question — "is it
 * working?" — and to make the answer "yes" with one click. Everything else this
 * app can do is behind "Chế độ nâng cao".
 */

const ACCOUNT_PAGE = "https://wisa.vn/account/api-connection";

function describeError(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Đã xảy ra lỗi không xác định.";
}

function KeyCard({ status }: { status: WisaStatus | null }) {
  const key = status?.key;
  return (
    <section className="rounded-xl border border-border-strong bg-surface p-4">
      <div className="flex items-start gap-3">
        <div className="mt-0.5 rounded-lg bg-action-soft p-2 text-action-text">
          <KeyRound className="h-4 w-4" strokeWidth={1.8} />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <h2 className="text-sm font-medium">Khoá WISA của bạn</h2>
            {key && (
              <Badge variant="outline" className="h-5 px-1.5 text-badge">
                {key.state === "active" ? "Đang hoạt động" : key.state}
              </Badge>
            )}
          </div>

          {key ? (
            <>
              <p className="mt-1 font-mono text-[13px] tracking-tight text-fg-2">
                {key.prefix}
                <span className="text-fg-3">
                  {"•".repeat(12)}
                  {key.fingerprint}
                </span>
              </p>
              <p className="mt-2 text-xs leading-relaxed text-fg-3">
                Một khoá dùng chung cho mọi client. Đổi khoá trên wisa.vn rồi
                bấm “Cấu hình lại” là tất cả client cập nhật theo.
              </p>
            </>
          ) : (
            <p className="mt-1 text-xs leading-relaxed text-fg-3">
              Chưa có khoá. Bấm “Cài tất cả” để WISA tạo khoá và cấu hình
              client.
            </p>
          )}

          <button
            type="button"
            className="mt-2 text-xs text-action-text underline-offset-2 hover:underline"
            onClick={() => void invoke("open_external", { url: ACCOUNT_PAGE })}
          >
            Quản lý khoá trên wisa.vn
          </button>
        </div>
      </div>
    </section>
  );
}

function ClientRow({
  client,
  busy,
  onConfigure,
  onUnlink,
}: {
  client: WisaClientStatus;
  busy: boolean;
  onConfigure: () => void;
  onUnlink: () => void;
}) {
  const stateLabel = client.connected
    ? "Đã kết nối"
    : client.installed
      ? "Chưa cấu hình"
      : "Chưa cài đặt";

  return (
    <div className="flex items-center gap-3 border-t border-border-strong px-4 py-3 first:border-t-0">
      <span className="text-fg-3">
        {client.connected ? (
          <CheckCircle2 className="h-4 w-4 text-success" strokeWidth={2} />
        ) : (
          <Circle className="h-4 w-4" strokeWidth={1.6} />
        )}
      </span>

      <div className="min-w-0 flex-1">
        <p className="text-sm font-medium">{client.label}</p>
        <p className="text-xs text-fg-3">{stateLabel}</p>
      </div>

      <div className="flex items-center gap-1">
        {client.connected && (
          <Button
            variant="ghost"
            size="sm"
            className="h-8 px-2 text-xs text-fg-3 hover:text-danger-text"
            disabled={busy}
            onClick={onUnlink}
          >
            Gỡ
          </Button>
        )}
        <Button
          variant={client.connected ? "ghost" : "outline"}
          size="sm"
          className="h-8 px-3 text-xs"
          disabled={busy}
          onClick={onConfigure}
        >
          {busy ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : client.connected ? (
            "Cấu hình lại"
          ) : (
            "Cấu hình"
          )}
        </Button>
      </div>
    </div>
  );
}

export function WisaHome() {
  const { session, refresh } = useWisaAuth();
  const [status, setStatus] = useState<WisaStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<WisaTargetId | "all" | null>(null);
  const [pendingUnlink, setPendingUnlink] = useState<WisaClientStatus | null>(
    null,
  );

  const load = useCallback(async () => {
    try {
      setStatus(await wisaApi.status());
    } catch (error) {
      toast.error(describeError(error));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const configure = useCallback(
    async (targets: WisaTargetId[]) => {
      setBusy(targets.length === 1 ? targets[0] : "all");
      try {
        const outcomes = await wisaApi.provision(targets);
        const failed = outcomes.filter((outcome) => !outcome.ok);
        const succeeded = outcomes.length - failed.length;
        if (succeeded > 0) {
          toast.success(
            targets.length > 1
              ? `Đã cấu hình ${succeeded} client. Mở lại client để dùng.`
              : `${outcomes[0]?.label ?? "Client"} đã sẵn sàng. Mở lại client để dùng.`,
          );
        }
        for (const outcome of failed) {
          toast.error(
            `${outcome.label}: ${outcome.message ?? "cấu hình thất bại"}`,
          );
        }
        await load();
      } catch (error) {
        toast.error(describeError(error));
      } finally {
        setBusy(null);
      }
    },
    [load],
  );

  const unlink = useCallback(async () => {
    const client = pendingUnlink;
    setPendingUnlink(null);
    if (!client) return;
    try {
      const [outcome] = await wisaApi.unlink([client.target]);
      if (outcome?.ok) {
        toast.success(`Đã gỡ ${client.label}.`);
      } else {
        toast.error(`${client.label}: ${outcome?.message ?? "gỡ thất bại"}`);
      }
      await load();
    } catch (error) {
      toast.error(describeError(error));
    }
  }, [pendingUnlink, load]);

  const signOut = useCallback(async () => {
    try {
      await wisaApi.logout();
    } finally {
      // Whether or not the local session could be cleared, the gate decides
      // what to show: re-reading is the only way to know.
      await refresh();
    }
  }, [refresh]);

  const clients = status?.clients ?? [];
  const connectedCount = clients.filter((client) => client.connected).length;
  const anyBusy = busy !== null;

  return (
    <>
      <AppPageHeader
        icon={<WisaMark className="h-5 w-5" />}
        title="WISA Connect"
        titleExtra={
          <Badge variant="outline" className="ms-1 h-5 px-1.5 text-badge">
            Beta
          </Badge>
        }
        actions={
          <>
            <Button
              variant="ghost"
              size="icon"
              className="h-8 w-8 text-fg-3"
              onClick={() => void load()}
              disabled={loading || anyBusy}
              title="Làm mới"
            >
              <RefreshCw
                className={`h-4 w-4 ${loading ? "animate-spin" : ""}`}
                strokeWidth={1.8}
              />
            </Button>
            <Button
              size="sm"
              className="h-8 px-3 text-xs"
              onClick={() => void configure([])}
              disabled={anyBusy}
            >
              {busy === "all" ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                "Cài tất cả"
              )}
            </Button>
          </>
        }
      />

      <div
        id="main-content"
        className="min-h-0 flex-1 overflow-y-auto scroll-stable"
      >
        <div className="mx-auto w-full max-w-2xl space-y-4 px-6 pb-10 pt-4">
          <section className="rounded-xl border border-border-strong bg-surface p-4">
            <div className="flex items-center justify-between gap-3">
              <div className="min-w-0">
                <p className="truncate text-sm font-medium">
                  {session.email ?? "Đã đăng nhập"}
                </p>
                <p className="text-xs text-fg-3">
                  {connectedCount > 0
                    ? `${connectedCount}/${clients.length} client đang dùng WISA`
                    : "Chưa client nào được cấu hình"}
                </p>
              </div>
              <Button
                variant="ghost"
                size="sm"
                className="h-8 gap-1.5 px-2 text-xs text-fg-3"
                onClick={() => void signOut()}
              >
                <LogOut className="h-3.5 w-3.5" strokeWidth={1.8} />
                Đăng xuất
              </Button>
            </div>
          </section>

          <KeyCard status={status} />

          <section className="overflow-hidden rounded-xl border border-border-strong bg-surface">
            <div className="px-4 py-3">
              <h2 className="text-sm font-medium">Client</h2>
              <p className="mt-0.5 text-xs text-fg-3">
                Cấu hình được ghi trực tiếp vào từng client, có sao lưu bản cũ.
              </p>
            </div>
            {loading && clients.length === 0 ? (
              <div className="flex items-center justify-center py-8">
                <Loader2 className="h-4 w-4 animate-spin text-fg-3" />
              </div>
            ) : (
              clients.map((client) => (
                <ClientRow
                  key={client.target}
                  client={client}
                  busy={busy === client.target || busy === "all"}
                  onConfigure={() => void configure([client.target])}
                  onUnlink={() => setPendingUnlink(client)}
                />
              ))
            )}
          </section>

          <p className="px-1 text-xs leading-relaxed text-fg-3">
            Sau khi cấu hình, hãy mở lại client đang chạy để nó đọc cấu hình
            mới. Toàn bộ giao diện gốc của ứng dụng nằm sau “Chế độ nâng cao” ở
            thanh bên trái.
          </p>
        </div>
      </div>

      <ConfirmDialog
        isOpen={pendingUnlink !== null}
        title={`Gỡ ${pendingUnlink?.label ?? ""} khỏi WISA?`}
        message="Cấu hình cũ của client này sẽ được khôi phục từ bản sao lưu. Khoá WISA trên tài khoản của bạn không bị ảnh hưởng."
        confirmText="Gỡ"
        onConfirm={() => void unlink()}
        onCancel={() => setPendingUnlink(null)}
      />
    </>
  );
}

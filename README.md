# WISA Connect

Đăng nhập bằng Google, và các AI client trên máy (Claude Code, Codex, Claude Desktop) được
cấu hình sẵn cho AI Gateway của WISA. Không terminal, không copy API key, không sửa JSON.

Đây là bản fork của [cc-switch](https://github.com/farion1231/cc-switch) (MIT) — xem
[`UPSTREAM.md`](./UPSTREAM.md) và [`LICENSE`](./LICENSE).

## Người dùng cuối

1. Tải `WISA Connect` từ https://wisa.vn/account/api-connection
2. Mở app → **Tiếp tục với Google**
3. Bấm **Cài tất cả** → app tự lấy key `ws_live_…`, tự ghi config, tự khởi động lại client

Trạng thái của từng client (chưa cài / đã cài nhưng chưa nối WISA / đang dùng WISA) nằm ở
trang chính. **Ngắt** khôi phục bản backup đã lưu trước khi ghi.

## Phát triển

Yêu cầu: Node 22 + pnpm 10 (pinned trong `package.json`), Rust 1.95 (`rust-toolchain.toml`),
và [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/) cho nền tảng của bạn.

```bash
pnpm install
pnpm dev            # Vite :3000 + cửa sổ Tauri
pnpm typecheck
pnpm test:unit
cd src-tauri && cargo clippy -- -D warnings && cargo test
```

`pnpm dev` dùng `strictPort`, nên port 3000 bận là lỗi ngay chứ không tự đổi port.

> Repo riêng, dùng **pnpm** (không dùng npm). Mọi lệnh chạy ở thư mục gốc repo này.

### Kết nối backend

| Việc | Endpoint |
|---|---|
| Google OAuth (PKCE, loopback) | `POST https://gateway.wisa.vn/studio/v1/auth/google/authorize` |
| Đổi code → JWT | `POST https://gateway.wisa.vn/studio/v1/auth/google` |
| Lấy key dùng chung | `GET  https://gateway.wisa.vn/studio/v1/account/api-key/secret` |
| Cấp key lần đầu | `POST /studio/v1/client-setup/issue` → `POST /studio/v1/client-setup/consume` |
| Danh sách model | `GET  https://ai.wisa.vn/v1/models` |

Mọi request đi từ Rust (`src-tauri/src/wisa/`), không từ webview: token không bao giờ vào JS,
và không phụ thuộc CORS của webview origin.

## Cấu trúc

```
src/                 React 18 + Vite + Tailwind + shadcn/ui
src-tauri/src/       Rust: provider engine, live config writers, proxy, wisa/ (auth + provisioning)
src-tauri/src/wisa/  Module WISA: auth.rs (PKCE), provision.rs, client.rs, keychain.rs
```

Trạng thái người dùng nằm ở `~/.wisa-connect/` (`wisa-connect.db`, `settings.json`, `backups/`).
Không dùng chung thư mục với cc-switch để hai app không ghi đè config của nhau.

## Trạng thái triển khai

Đã chạy được (đã kiểm bằng `pnpm typecheck`, `cargo check`, `cargo test`):

- Đăng nhập Google qua PKCE + loopback `127.0.0.1`, refresh token nằm trong Credential
  Manager (Windows/macOS) hoặc tệp `0600` ở nơi khác. Access token chỉ sống trong Rust.
- Trang **WISA Connect** (`wisa`): trạng thái từng client, khoá đang dùng, **Cài tất cả**,
  **Cấu hình lại**, **Gỡ**, **Đăng xuất**.
- Provisioning: đọc key qua `GET /account/api-key/secret` (fallback `issue` + `consume`),
  lấy model từ `GET /v1/models`, upsert provider id `wisa-gateway` rồi `switch` — nên phần
  ghi config, backup và khôi phục là của provider engine sẵn có, không phải code mới.
- **Chế độ nâng cao** (mặc định tắt) mở lại toàn bộ bề mặt cc-switch: provider, MCP, prompt,
  proxy, usage, phiên. Bật/tắt được ở thanh bên, ghi nhớ theo máy.
- Giao diện mặc định tiếng Việt (`src/i18n/locales/vi.json`, thiếu key thì rơi về tiếng Anh),
  icon và tên sản phẩm đã đổi, updater trỏ `assets.winsell.vn/wisa-connect`.

Logo lấy nguyên bản từ `apps/web/public/branding/wisa-logo-badge.png` của repo web, chép
nguyên byte vào `src/assets/brand/` — không vẽ lại. Muốn đổi icon thì sửa tệp nguồn ở repo
web, chép lại, rồi chạy:

```powershell
pnpm tauri icon src/assets/brand/wisa-logo-badge.png
```

Lệnh đó ghi đè toàn bộ `src-tauri/icons/`, trừ ba ảnh tray macOS
(`src-tauri/icons/tray/macos/`), vốn là ảnh template một màu nên phải tạo riêng từ
`src/assets/brand/wisa-logo.png`.

Chưa xong:

- **Chưa có chứng chỉ Authenticode.** Thiếu nó, SmartScreen sẽ chặn lần cài đầu — rào cản
  lớn hơn mọi thứ còn lại trong danh sách này.
- **`vi.json` mới dịch phần lõi** (nav + tên ngôn ngữ). Phần còn lại rơi về tiếng Anh.
- Claude Desktop: provider ghi theo dạng `env` trực tiếp; chưa kiểm chứng trên máy có
  Claude Desktop thật.
- **Chưa chạy thử end-to-end**: đăng nhập Google thật + cài đặt lên máy sạch.

### Phát hành bản mới

Khoá ký updater đã là của WISA: public key nằm trong `src-tauri/tauri.conf.json`, private
key **không** nằm trong repo (`.gitignore` chặn `*.key`, `*.key.pub`). Để phát hành:

1. Đặt các secret sau ở **Settings → Secrets and variables → Actions**:

   | Secret | Bắt buộc | Dùng cho |
   |---|---|---|
   | `TAURI_SIGNING_PRIVATE_KEY` | có | nội dung tệp private key vừa tạo |
   | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | nếu có đặt mật khẩu | mở private key |
   | `CLOUDFLARE_API_TOKEN`, `CLOUDFLARE_ACCOUNT_ID` | để publish R2 | `wrangler r2 object put` |
   | `R2_BUCKET` | để publish R2 | tên bucket của `assets.winsell.vn` |
   | `WINDOWS_CERTIFICATE[_PASSWORD]` | khi có chứng chỉ | Authenticode |
   | `APPLE_*` | khi có chứng chỉ | ký và notarize macOS |

2. Chạy workflow **Release WISA Connect** (`workflow_dispatch`) với `version`, ví dụ `0.1.0`.
   Workflow build cả Windows và macOS, kiểm tra `TAURI_SIGNING_PRIVATE_KEY` có tồn tại rồi
   tạo GitHub Release ở dạng **draft**.
3. Publish lên R2 để updater thấy bản mới:

   ```powershell
   node scripts/publish-r2-release.mjs .\release-artifacts v0.1.0 --bucket $env:R2_BUCKET
   ```

   Script từ chối chạy nếu thiếu `latest.json` hoặc thiếu `.sig` cho bất kỳ mục nào trong
   manifest, nên không thể publish một bản mà client sẽ từ chối. Thêm `--dry-run` để xem
   trước. Manifest được viết lại sang URL R2 bởi `scripts/rewrite-updater-manifest.mjs`;
   chữ ký minisign phủ nội dung tệp chứ không phủ URL nên vẫn hợp lệ.

## Repo công khai

Repo: **https://github.com/winsell-vn/wisa-connect** (public, thuộc org `winsell-vn`).
Secret scanning và push protection đã bật.

- **Không commit secret nào.** `TAURI_SIGNING_PRIVATE_KEY`, `WINDOWS_CERTIFICATE`, `APPLE_*`
  nằm ở GitHub Actions Secrets. Backend (`apps/api`) và web vẫn ở repo riêng, private.
- `LICENSE` (MIT) và `UPSTREAM.md` phải giữ: đây là nghĩa vụ khi fork cc-switch, không phải
  tuỳ chọn.
- Source công khai **không** lộ bí mật server: `GOOGLE_CLIENT_SECRET`, JWT signing key và
  `ws_live_…` của khách đều không có trong client. Client chỉ giữ refresh token của chính
  người dùng, trong Credential Manager.
- Rủi ro thật của việc công khai là **installer giả**, không phải lỗ hổng trong code: ai
  cũng build lại được từ source này. Chặn bằng chữ ký Authenticode và **chỉ phát hành từ
  `assets.winsell.vn`** — người dùng chỉ nên tải từ trang đó.

## Giấy phép

MIT. Bản quyền gốc của cc-switch thuộc về Jason Young và các contributor — xem `LICENSE`.

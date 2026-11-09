# Upstream: cc-switch

`wisa-connect` là bản fork của **cc-switch** (MIT), không phải mã nguồn gốc của WISA.

| | |
|---|---|
| Upstream | https://github.com/farion1231/cc-switch |
| Tag gần nhất | `v4.0.27` |
| Commit đã vendor | `b184a88c99428571d718d10d1a1403e9502cd28b` (`v4.0.27-10-gb184a88c`, 2026-11-08) |
| Ngày vendor | 2026-11-08 |
| Giấy phép | MIT — giữ nguyên `LICENSE`, không xoá copyright notice |

## Đã thay đổi so với upstream

Cơ học (đổi tên hàng loạt, không đổi hành vi):

- `cc_switch_lib` → `wisa_connect_lib`, crate/binary `cc-switch` → `wisa-connect`
- identifier `com.ccswitch.desktop` → `vn.wisa.connect`, productName `WISA Connect`
- thư mục trạng thái `~/.cc-switch` → `~/.wisa-connect`
- deep-link `ccswitch://` → `wisa-connect://`
- updater endpoint → `https://assets.winsell.vn/wisa-connect/latest.json`

Có chủ đích (không merge ngược được):

- `src-tauri/src/wisa/` — module mới: Google OAuth loopback, keychain, provisioning
- UI: `LoginGate` + `StatusHome`; UI gốc của cc-switch nằm sau "Chế độ nâng cao"
- i18n: thêm `vi`, mặc định `vi`
- xoá `docs/`, `.github/`, `CHANGELOG.md`, `deplink.html`, các README ngôn ngữ khác

## Đồng bộ upstream

Không tự động. Upstream đang active, nên mỗi lần sync là một quyết định có chủ đích:

1. `git clone https://github.com/farion1231/cc-switch /tmp/cc-switch-upstream`
2. `git -C /tmp/cc-switch-upstream log --oneline b184a88c..HEAD` → đọc danh sách thay đổi
3. Chọn từng commit đáng lấy; port tay vào repo này (đừng merge — lịch sử hai repo
   khác nhau, và phần đã đổi tên sẽ conflict hàng loạt)
4. Cập nhật bảng trên sau khi xong

Ưu tiên port: sửa lỗi ghi config (`live/`, `services/provider/`), thêm client mới, bảo mật.
Bỏ qua: rebrand, tài liệu, CI, tính năng thương mại của upstream.

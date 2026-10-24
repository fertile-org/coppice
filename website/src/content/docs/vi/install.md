---
title: Cài đặt
description: "Coppice Beta là ứng dụng desktop. Không cần Docker, không cần server, không cần tài khoản Coppice."
---

# Cài đặt

Coppice Beta là ứng dụng desktop. Không cần Docker, không cần server, không cần tài khoản Coppice.

<a id="supported-platforms"></a>

## Nền tảng hỗ trợ
| Nền tảng | Gói cài |
| --- | --- |
| macOS (Apple Silicon) | `.dmg` |
| Linux (x64) | `.deb` |

Bản beta này chưa có ứng dụng cho Windows. Nếu dùng Windows 11, xem [Windows 11 (qua WSL)](#windows-11-through-wsl).

<a id="download"></a>

## Tải về
- [macOS (Apple Silicon) `.dmg`](https://github.com/fertile-org/coppice/releases/latest/download/Coppice-mac-arm64.dmg)
- [macOS (Intel) `.dmg`](https://github.com/fertile-org/coppice/releases/latest/download/Coppice-mac-x64.dmg)
- [Linux (x64) `.deb`](https://github.com/fertile-org/coppice/releases/latest/download/Coppice-linux-amd64.deb)
- [Linux (ARM64) `.deb`](https://github.com/fertile-org/coppice/releases/latest/download/Coppice-linux-arm64.deb)

Mọi bản build đều có trên [GitHub Releases](https://github.com/fertile-org/coppice/releases).

<a id="updates"></a>

## Cập nhật

Coppice tự kiểm tra phiên bản mới ở chế độ nền. Muốn tắt thì vào **System → Updates** và tắt **Automatic updates**. Cập nhật xong, các bảng và thiết lập vẫn còn nguyên.

- **macOS:** Coppice tự tải bản cập nhật về. Bấm **Restart to update** trên banner, hoặc để nó tự cài vào lần tới bạn thoát Coppice.
- **Linux:** khi cài file `.deb`, nguồn gói của Coppice được thêm vào máy, nên bản mới sẽ đến cùng các bản cập nhật khác trong Software Updater. Hoặc chạy `sudo apt update && sudo apt upgrade`.

<a id="windows-11-through-wsl"></a>

## Windows 11 (qua WSL)
Bản Linux x64 có thể chạy trên Windows 11 qua WSL, nhưng chúng tôi chưa thử. Nếu bạn thử, hãy [kể lại kết quả cho chúng tôi](https://github.com/fertile-org/coppice/issues).

Cần Windows 11, vì các bản Windows 10 cũ không hiện được cửa sổ ứng dụng Linux. Máy Windows chạy chip Arm thì hiện chưa hỗ trợ.

1. Mở PowerShell bằng quyền administrator và chạy `wsl --install`. Lệnh này sẽ cài Ubuntu. Khởi động lại máy khi được hỏi, rồi tạo user Ubuntu cho xong.
2. Vẫn trong PowerShell quyền administrator, cập nhật WSL để cửa sổ ứng dụng Linux hiện được trên desktop Windows:

   ```powershell
   wsl --update
   wsl --shutdown
   ```

3. Mở Ubuntu từ menu Start. Các bước còn lại đều làm trong Ubuntu. Tải file `.deb` bản Linux x64 rồi cài. Thay `<version>` bằng phiên bản mới nhất trên [GitHub Releases](https://github.com/fertile-org/coppice/releases):

   ```sh
   cd ~
   wget https://github.com/fertile-org/coppice/releases/download/v<version>/Coppice-<version>-linux-amd64.deb
   sudo apt install ./Coppice-<version>-linux-amd64.deb
   ```

   Hãy dùng `apt`, đừng dùng `dpkg -i`. `apt` sẽ cài luôn những gói Coppice cần.

   Sau này muốn cập nhật Coppice, chạy `sudo apt update && sudo apt upgrade`. Các bảng và thiết lập vẫn còn nguyên.
4. Cài các agent CLI bạn dùng (Claude Code, Codex, Cursor, OpenCode, Kilo Code, GitHub Copilot, Qwen Code, Kimi Code, Grok) trong Ubuntu. Đăng nhập các CLI đó và git cũng ngay trong Ubuntu.
5. Mở Coppice từ menu Start của Windows (trong mục Ubuntu), hoặc chạy `coppice` trong Ubuntu.

   Nếu một CLI báo Not on your PATH, hãy mở Coppice bằng lệnh `coppice` trong Ubuntu.

WSL đưa PATH của Windows vào Ubuntu, nên Coppice có thể tìm nhầm bản CLI cài trên Windows. Vào **System → Connectors** và xem đường dẫn CLI của từng connector. Đường dẫn nào bắt đầu bằng `/mnt/c/` thì cài lại CLI đó trong Ubuntu.

Hãy để repo trong thư mục home của Linux, ví dụ `~/code/my-app`, đừng để trong `/mnt/c/`. Git và việc đọc ghi file qua ranh giới đó chậm hơn nhiều. Hộp chọn thư mục sẽ hiện đường dẫn Linux.

<a id="first-run"></a>

## Lần đầu mở ứng dụng
1. Cài và mở Coppice.
2. Chọn một thư mục git checkout có sẵn trên máy.
3. **Kết nối agent CLI.** Khi tạo agent, chọn agent CLI muốn dùng, Coppice sẽ bật nó lên. Mỗi CLI hiện một trong các trạng thái **Ready**, **Found, not signed in** hoặc **Not on your PATH**. Nhớ đăng nhập CLI trong terminal trước, như bạn vẫn làm.

   Kilo Code, GitHub Copilot, Qwen Code và Kimi Code hiện **Installed** (ví dụ **Kilo Code — Installed**). Trong System → Connectors có thêm dòng: "Coppice can't check this sign-in without using your plan. A run will show an error if you're signed out." (Coppice không kiểm tra được trạng thái đăng nhập nếu không dùng đến gói của bạn. Nếu bạn đã đăng xuất, lần chạy sẽ báo lỗi.)

   Sau này muốn tắt một connector, dùng công tắc trong System → Connectors, hoặc đặt `enabled = false` cho nó trong file `config.toml` của Coppice:

   - macOS: `~/Library/Application Support/Coppice/config.toml`
   - Linux: `~/.config/Coppice/config.toml`

4. Tạo một ticket (hoặc duyệt một ticket được đề xuất) rồi kéo sang Ready. Các agent sẽ đưa nó qua In Progress, In Review và In QA, sau đó ticket chờ bạn ở bước Human Review.

<a id="requirements"></a>

## Yêu cầu
- Một git repository trên máy mà bạn có quyền ghi (Coppice làm việc trong các worktree riêng)
- Ít nhất một agent CLI được hỗ trợ, đã cài và đăng nhập theo cách thông thường của công cụ đó

<a id="troubleshooting"></a>

## Xử lý sự cố
Nếu ứng dụng không mở lên hoặc agent mãi không nhận ticket, hãy xem terminal trực tiếp trên ticket và trang Hỏi đáp. Muốn tự build từ mã nguồn, xem [tài liệu cho người đóng góp](https://github.com/fertile-org/coppice/blob/main/docs/development.md) trong repo.

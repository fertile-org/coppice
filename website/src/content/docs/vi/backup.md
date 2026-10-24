---
title: Sao lưu dữ liệu
metaTitle: "Sao lưu và chuyển dữ liệu Coppice"
description: "Tải bản sao lưu Coppice, nhập vào máy mới, chép thư mục dữ liệu bằng tay, và thay khóa nếu bản sao lưu bị lộ."
---

# Sao lưu dữ liệu

<a id="download-a-backup"></a>

## Tải bản sao lưu

Mở **System**, rồi **Backup**, và chọn **Download backup**. Bạn sẽ có một file chứa toàn bộ bảng, ticket, thiết lập, file và worktree của agent.

File này không có mật khẩu, khóa và token đã lưu, nhưng vẫn chứa toàn bộ dữ liệu workspace, nên đừng chia sẻ lung tung.

<a id="import-a-backup"></a>

## Nhập bản sao lưu

Trên máy mới, mở **System**, rồi **Backup**, và chọn **Import backup**.

Nhập bản sao lưu sẽ ghi đè toàn bộ dữ liệu trên máy đó. Bảng và thiết lập workspace được chuyển sang, còn mật khẩu, khóa, đường dẫn thư mục, lệnh của agent, thiết lập plugin và MCP thì không, nên bạn cần thiết lập lại. Bạn có thể sẽ bị đăng xuất.

Bản sao lưu không có token đã lưu, nên sau khi nhập bạn cần dán lại từng token. Token nào cần dán lại sẽ có dòng "Paste this token again." Dán vào rồi lưu là xong.

<a id="copy-the-folders-by-hand"></a>

## Tự sao chép thư mục

Bạn cũng có thể thoát Coppice rồi tự sao chép thư mục dữ liệu:

| | Dữ liệu | Log |
| --- | --- | --- |
| macOS | `~/Library/Application Support/Coppice` | `~/Library/Logs/Coppice` |
| Linux | `~/.config/Coppice` | `~/.local/state/Coppice/logs` |

Bản sao này có cả thư mục `secrets`, nên hãy giữ nó cẩn thận như mật khẩu. Thông tin đăng nhập của các CLI thì không nằm ở đây, mỗi CLI tự lưu riêng.

<a id="replace-keys"></a>

## Thay khóa

Nếu bạn đã lỡ chia sẻ bản sao lưu, hoặc nghi người khác đang giữ một bản, hãy tạo khóa mới.

Mở **System**, rồi **Backup**, và chọn **Replace keys**. Nhập mật khẩu admin mới và đánh dấu ô xác nhận. Coppice sẽ tạo khóa và mật khẩu database mới. Token đã lưu vẫn dùng được, nhưng mọi người sẽ bị đăng xuất.

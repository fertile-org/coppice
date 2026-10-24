---
title: Phím tắt
metaTitle: "Phím tắt bàn phím"
description: "Dùng Coppice bằng bàn phím: di chuyển trên bảng, mở và chuyển ticket, review diff không cần chuột, và tự đặt phím tắt riêng."
---

# Phím tắt

Nhấn `?` ở bất kỳ đâu để xem toàn bộ phím tắt. Phím tắt được chia thành các nhóm **Navigate**, **Tickets** và **Review**. Đổi phím tắt trong Settings.

`Mod` là `⌘` trên macOS và `Ctrl` trên Linux và WSL. Chuỗi phím được ghi dạng `G then B` (nhấn G, rồi nhấn B).

Phím tắt không có tác dụng khi bạn đang gõ trong ô nhập, và terminal có bộ phím riêng. Nhấn `Esc` để đóng thứ đang mở.

<a id="navigate"></a>

## Navigate (di chuyển)

| Thao tác | Id | Phím |
| --- | --- | --- |
| Mở bảng lệnh | `palette.open` | `Mod+K` |
| Hiện các phím tắt | `help.show` | `?` |
| Đóng | `close` | `Esc` |
| Thẻ tiếp theo | `board.nextCard` | `J` hoặc `Down` |
| Thẻ trước | `board.previousCard` | `K` hoặc `Up` |
| Cột tiếp theo | `board.nextColumn` | `L` hoặc `Right` |
| Cột trước | `board.previousColumn` | `H` hoặc `Left` |
| Đến cột 1 | `board.column.1` | `G then 1` |
| Đến cột 2 | `board.column.2` | `G then 2` |
| Đến cột 3 | `board.column.3` | `G then 3` |
| Đến cột 4 | `board.column.4` | `G then 4` |
| Đến cột 5 | `board.column.5` | `G then 5` |
| Đến cột 6 | `board.column.6` | `G then 6` |
| Đến cột 7 | `board.column.7` | `G then 7` |
| Đến cột 8 | `board.column.8` | `G then 8` |
| Đến cột 9 | `board.column.9` | `G then 9` |
| Đến Boards | `nav.boards` | `G then B` |
| Đến Agents | `nav.agents` | `G then A` |
| Đến Chat | `nav.chat` | `G then C` |
| Đến Knowledge | `nav.knowledge` | `G then K` |
| Đến Repositories | `nav.repositories` | `G then R` |
| Đến Plugins | `nav.plugins` | `G then P` |
| Đến Users | `nav.users` | `G then U` |
| Đến System | `nav.tools` | `G then T` |
| Đến Settings | `nav.settings` | `G then S` |
| Phần tiếp theo | `drawer.nextSection` | `]` |
| Phần trước | `drawer.previousSection` | `[` |

**Đến cột 1** tới **Đến cột 9** đi theo bảng từ trái sang phải: 1 Backlog, 2 Plan Review, 3 Ready, 4 In Progress, 5 In Review, 6 In QA, 7 Wait for Human Review, 8 Done, 9 Blocked.

**Đến Plugins**, **Đến Users**, **Đến System** và **Đến Settings** không hiện với thành viên thường, giống như trên thanh bên. Ứng dụng desktop cũng không có **Đến Users**.

**Thẻ tiếp theo** và **Thẻ trước** chỉ di chuyển trong một cột và không quay vòng lại đầu. **Cột tiếp theo** và **Cột trước** dừng ở thẻ cùng vị trí, ở thẻ cuối, hoặc ở tiêu đề cột khi cột đó trống. **Phần tiếp theo** và **Phần trước** chuyển drawer đang mở qua lại giữa Detail, Live Console và Agent Runs.

<a id="tickets"></a>

## Tickets

| Thao tác | Id | Phím | Khi nào |
| --- | --- | --- | --- |
| Mở ticket | `ticket.open` | `Enter` | Đang chọn một thẻ |
| Tạo ticket | `ticket.create` | `C` | Ở bảng, không mở drawer |
| Giao ticket | `ticket.assign` | `A` | Đang chọn một thẻ hoặc drawer đang mở |
| Chuyển sang cột tiếp theo | `ticket.moveNext` | `M then L` | Đang chọn một thẻ hoặc drawer đang mở |
| Chuyển về cột trước | `ticket.movePrevious` | `M then H` | Đang chọn một thẻ hoặc drawer đang mở |
| Gửi bình luận | `ticket.comment` | `Mod+Enter` | Ô soạn bình luận đang được chọn và có nội dung |

**Tạo ticket** đưa con trỏ vào ô **Add ticket…** của cột đang chọn, nhấn Enter để tạo. Ô này nằm ở cột Backlog.

**Giao ticket** mở ô chọn người được giao. Nếu drawer đang đóng, ticket sẽ mở ra trước rồi mới tới ô chọn.

**Chuyển sang cột tiếp theo** và **Chuyển về cột trước** đi theo thứ tự cột ở trên.

<a id="review"></a>

## Review

| Thao tác | Id | Phím | Khi nào |
| --- | --- | --- | --- |
| Duyệt kế hoạch | `review.approvePlan` | `R then P` | Ticket ở Plan Review và **Approve plan** đang bật |
| Accept… | `review.accept` | `Mod+Shift+Enter` | Ticket ở Wait for Human Review và chưa bị archive |
| Merge… | `review.merge` | `R then M` | **Merge…** đang hiện |
| Mở code review | `review.openCode` | `D` | **Open code review** đang bật |

**Accept…** mở hộp thoại **Accept this ticket?** với nút **Cancel** được chọn sẵn. Chọn **Accept** để hoàn tất.

**Merge…** mở hộp thoại **Merge ticket branch**.

<a id="a-pass-with-only-the-keyboard"></a>

## Một vòng chỉ dùng bàn phím

1. Trên bảng, nhấn `C`. Ô **Add ticket…** được chọn. Gõ tiêu đề và nhấn Enter.
2. Di chuyển tới thẻ đó bằng `J` hoặc `K`, rồi nhấn `A`. Ô chọn người được giao mở ra.
3. Nhấn Enter.
4. Chọn ô soạn bình luận, gõ nội dung, và nhấn `Mod+Enter`.
5. Nhấn `D`.
6. Khi ticket ở Wait for Human Review, nhấn `Mod+Shift+Enter`. **Cancel** được chọn sẵn. Chọn **Accept** để hoàn tất.
7. Nhấn `Esc`. Con trỏ quay lại thẻ trên bảng.

<a id="custom-shortcuts"></a>

## Tùy chỉnh phím tắt

Bạn đổi được mọi phím tắt trong Settings, ở mục `[keybindings]`, dùng id lệnh trong các bảng ở trên. Phím bạn đặt sẽ thay phím mặc định, còn để trống là tắt phím tắt đó.

Nếu phím tắt viết sai, trỏ tới lệnh không tồn tại, trùng phím với lệnh khác, hoặc là phím của trình duyệt hay hệ thống, lệnh đó sẽ giữ phím mặc định và Settings sẽ ghi rõ lý do. Bảng trợ giúp phím tắt và bảng lệnh luôn hiện đúng các phím đang dùng.

```toml
[keybindings]
"palette.open" = "P"
"ticket.create" = ""
```

Mở **Settings**, sửa file rồi bấm **Save**. Phím mới có hiệu lực ngay, không cần khởi động lại.

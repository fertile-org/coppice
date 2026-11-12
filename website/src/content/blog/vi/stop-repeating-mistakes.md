---
title: Cách để agent lập trình không lặp lại cùng một lỗi
metaTitle: Agent lập trình không lặp lại lỗi
description: "Đặt phần sửa vào file quy tắc, kiểm tra sẽ thất bại, việc đang làm, hoặc skill. Trong Coppice, duyệt ghi chú Knowledge để ticket khớp liệt kê nó."
answer: "Viết phần sửa ở chỗ phiên tiếp theo sẽ thấy. File quy tắc như CLAUDE.md, AGENTS.md, hoặc một quy tắc Cursor được nạp vào các phiên sau. Test hoặc kiểm tra lint thất bại khi lỗi quay lại. Bạn cũng có thể đặt vào việc đang làm, hoặc vào skill mà agent nạp khi việc hợp. Trong Coppice, lưu thành ghi chú Knowledge và duyệt. Ở ticket sau có từ khớp, agent được bảo đọc ghi chú đó."
published: "2026-11-11"
author: Coppice
authorType: Organization
draft: false
keywords:
  - agent lập trình
  - CLAUDE.md
  - AGENTS.md
  - quy tắc Cursor
  - Knowledge
---

## Vì sao việc này khó

Bạn sửa cho agent. Việc tiếp theo bắt đầu từ đầu, và lỗi lại xuất hiện. Agent lập trình không giữ lần sửa đó khi chat kết thúc. Phải có thứ đặt nó trước lần chạy tiếp theo, hoặc một kiểm tra phải thất bại khi lỗi quay lại.

## Cách 1: file quy tắc trong dự án

Mỗi phiên bắt đầu, Claude Code đọc `CLAUDE.md`, từ thư mục bạn khởi động và các thư mục phía trên. Khi đường dẫn đó không có `CLAUDE.md` cũng không có `CLAUDE.local.md`, nó đọc `AGENTS.md`. Nó cũng nạp 200 dòng đầu của file bộ nhớ tự động `MEMORY.md`, hoặc 25KB, cái nào tới trước. Claude Code coi các file này là ngữ cảnh. Chúng không chặn một lần sửa.

Codex đọc `AGENTS.md` trước khi làm. Nó đi từ gốc dự án xuống thư mục bạn khởi động, và dừng khi các file gộp lại đạt 32 KiB.

Cursor đọc `AGENTS.md` ở gốc dự án và trong thư mục con của các file bạn đang làm. Một quy tắc dự án trong `.cursor/rules` có trong mọi chat khi bạn đặt always apply. Nếu không, Cursor đưa nó vào khi file khớp, khi agent thấy nó hợp, hoặc khi bạn nhắc tới. File quy tắc là lời khuyên. Agent có thể bỏ qua một dòng.

## Cách 2: test hoặc kiểm tra lint

Khi một kiểm tra nhìn thấy lỗi, hãy thêm kiểm tra đó. Một test thất bại khi control trong Settings tự giữ trạng thái theme, hoặc một quy tắc lint cho kiểu đó, sẽ chặn thay đổi ở lần chạy sau.

Cách này chặn lỗi. Một lựa chọn bố cục vẫn cần người, vì kiểm tra không thấy được.

## Cách 3: viết vào việc, hoặc vào skill

Dán phần sửa vào việc này. Agent thấy nó trong lần chạy này. Việc sau phải dán lại.

Skill là file hướng dẫn ngắn mà agent nạp khi việc hợp, hoặc khi bạn gọi nó. Trong Claude Code, file nằm ngoài phiên cho tới lúc đó, nên một quy tắc một dòng thuộc về `CLAUDE.md` và một quy trình thuộc về skill. Agent vẫn có thể bỏ qua.

## Cách 4: ghi chú Knowledge bạn duyệt, với Coppice

[Coppice](/) là ứng dụng desktop đặt các agent lập trình lên một bảng việc. Knowledge là ghi chú về dự án cho các ticket sau. Bạn viết ghi chú, hoặc trong một ticket, agent có thể đề xuất một ghi chú cho quy tắc mà ticket sau sẽ cần. Nó ở trạng thái chờ cho tới khi bạn duyệt. Các lần chạy sau không thấy ghi chú đang chờ.

Một ghi chú có thể phủ mọi bảng, một bảng, hoặc một agent trên một bảng. Ghi chú mà agent đề xuất từ một ticket phủ bảng đó.

1. [Cài Coppice](/docs/install) và thêm dự án.
2. Mở Knowledge và thêm một ghi chú. Với công tắc dark mode trong Settings, viết rằng control dùng theme hook sẵn có, và thay đổi gồm một bài test phần thiết lập. Chọn bảng của việc đó.
3. Duyệt ghi chú.
4. Ở ticket tiếp theo, cho agent chạy. Coppice tìm trong các ghi chú đã duyệt những từ ở tiêu đề và mô tả ticket. Một kết quả khớp được liệt kê cho agent lúc bắt đầu chạy, và agent được bảo đọc ghi chú đó trước khi dựa vào nó.

Một ticket sau có tiêu đề Add a dark mode toggle to Settings dùng chung các từ đó, nên ghi chú được liệt kê. Toàn văn nằm ngoài prompt cho tới khi agent đọc. Agent cũng có thể tìm các ghi chú đã duyệt. Terminal ngoài Coppice không bao giờ thấy ghi chú. [Khái niệm](/docs/concepts#knowledge) giải thích Knowledge.

![Hộp thoại thêm Knowledge. Tiêu đề là Settings theme controls use the existing theme hook, loại là Review feedback, bảng là acme-web, và ghi chú nói công tắc dark mode dùng theme hook sẵn có và gồm một bài test phần thiết lập.](/blog/stop-repeating-mistakes/add-note.png "Ghi chú nêu theme hook, trên bảng acme-web.")

![Một ghi chú Knowledge đang chờ trên bảng acme-web. Tiêu đề là Settings theme controls use the existing theme hook, và Approve nằm trên thẻ.](/blog/stop-repeating-mistakes/pending-note.png "Ghi chú ở trạng thái chờ cho tới khi bạn duyệt.")

## Chọn cách nào

- **Lỗi có thể làm một kiểm tra thất bại:** thêm test hoặc quy tắc lint.
- **Mỗi phiên của một công cụ nên thấy một quy tắc ngắn:** `CLAUDE.md`, `AGENTS.md`, hoặc một quy tắc Cursor.
- **Một việc, hoặc một quy trình bạn không muốn trong mọi phiên:** viết vào việc, hoặc đặt vào skill.
- **Nhiều ticket, và bạn muốn duyệt phần được giữ trước khi lần chạy sau dùng được:** Coppice. Xem [Bắt đầu](/docs/getting-started).

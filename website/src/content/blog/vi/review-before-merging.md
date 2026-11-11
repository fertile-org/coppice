---
title: Cách review code do AI viết trước khi merge
metaTitle: Duyệt code AI trước khi merge
description: "Trước khi merge, cần xem gì trong diff của agent, và ba cách review: trong git, nhờ agent thứ hai, hoặc duyệt kế hoạch trước."
answer: "Đối chiếu diff với điều bạn đã yêu cầu: file không được đụng, xử lý lỗi, dependency mới, và test thật sự phủ thay đổi. Bạn có thể review trong git hoặc pull request, nhờ một agent khác xem trước, hoặc dùng bảng như Coppice, nơi bạn duyệt kế hoạch trước khi có code và review diff theo kế hoạch đó trước khi merge."
published: "2026-11-07"
updated: "2026-11-09"
author: Coppice
authorType: Organization
draft: false
keywords:
  - review code AI
  - coding agent
  - review code
  - merge
---

## Vì sao việc này khó

Agent viết code nhanh hơn bạn đọc. Một yêu cầu nhỏ có thể trở về thành diff lớn, kèm thay đổi bạn không yêu cầu, và phần tóm tắt nói mọi thứ đều chạy. Đọc từng dòng thì quá lâu. Lướt qua thì lỗi lọt.

Phần khó là biết agent đáng lẽ phải làm gì. Không có điều đó, bạn không phân biệt được một thay đổi có thuộc diff hay là phạm vi không ai yêu cầu.

## Cần xem gì trong diff

Dù review theo cách nào, hãy xem những điểm này:

- **Phạm vi.** File bạn không ngờ tới, chẳng hạn lockfile, migration, cấu hình, hoặc đổi tên.
- **Đường lỗi.** Chuyện gì xảy ra khi thiếu input, một lời gọi thất bại, hoặc giá trị rỗng.
- **Dependency và quyền truy cập.** Package, script, thông tin đăng nhập mới, hoặc thay đổi về ai được đọc hay sửa dữ liệu.
- **Test.** Test thật sự chạy qua thay đổi, không chỉ một dòng trong phần tóm tắt nói test đã qua.

## Cách 1: review trong git hoặc pull request

Diff nhánh của agent với nhánh chính, hoặc mở pull request và review như công việc của một đồng nghiệp. Cách này dùng được với mọi agent và mọi công cụ bạn đang dùng.

Giới hạn là bạn chỉ thấy kết quả. Điều bạn yêu cầu nằm trong lịch sử chat, nên bạn đối chiếu diff với trí nhớ về yêu cầu đó.

## Cách 2: nhờ một agent khác review trước

Trước khi tự xem, hãy nhờ một agent khác review diff, ví dụ Codex review phần Claude Code viết. Agent đó bắt lỗi rõ ràng và đưa danh sách chỗ cần nhìn.

Review của agent đó là ý kiến thứ hai, không phải chữ ký duyệt. Bạn vẫn đọc diff và tự quyết định.

## Cách 3: duyệt kế hoạch trước, với Coppice

[Coppice](/) là ứng dụng desktop đặt các agent lập trình lên một bảng việc. Trước khi có code, một agent viết kế hoạch và bạn duyệt. Khi việc xong, Coppice hiện diff cạnh kế hoạch đã duyệt và đánh dấu file nào kế hoạch không nhắc tới. Chấp nhận công việc không có nghĩa là merge, nên bạn chọn lúc nào nó vào nhánh chính.

1. [Cài Coppice](/docs/install) và thêm dự án.
2. Viết một ticket và cho agent chạy. Agent PM viết kế hoạch.
3. Đọc kế hoạch. Duyệt kế hoạch, hoặc yêu cầu sửa.
4. Khi ticket tới Wait for Human Review, review code. Bình luận trên dòng, hoặc chấp nhận công việc.
5. Merge khi bạn sẵn sàng.

Trông như thế này. Ticket yêu cầu một công tắc dark mode trong Settings. Kế hoạch của agent PM có ba bước: thêm công tắc, dùng theme hook sẵn có, và thêm một bài test phần thiết lập.

![Một ticket tiêu đề Add a dark mode toggle to Settings, đang ở Plan Review. Kế hoạch đề xuất làm một công tắc trên dòng Settings, với ba bước: thêm công tắc Settings, dùng theme hook sẵn có, và thêm một bài test phần thiết lập.](/blog/review-before-merging/plan-review.png "Kế hoạch, trước khi có code.")

Khi việc quay lại, kế hoạch đã duyệt nằm cạnh diff. Có hai điểm nổi bật. Component mới tự giữ trạng thái theme thay vì dùng theme hook sẵn có, và chỉ một file thay đổi, nên không có bài test phần thiết lập. Cả hai đều đáng viết một bình luận trước khi bạn chấp nhận.

![Màn hình review của cùng ticket. Kế hoạch đã duyệt ở bên trái, src/settings/ThemeToggle.tsx là file duy nhất thay đổi, và một bình luận dưới dòng useState nói hãy dùng theme hook sẵn có.](/blog/review-before-merging/diff-before-accept.png "Diff cạnh kế hoạch đã duyệt, với bình luận trên dòng bỏ qua theme hook.")

[Khái niệm](/docs/concepts) giải thích từng bước một ticket đi qua.

## Chọn cách nào

- **Một thay đổi nhỏ từ một agent:** review trong git hoặc pull request.
- **Thay đổi lớn hơn, và bạn muốn có người xem trước:** nhờ một agent khác, rồi tự đọc.
- **Nhiều việc của agent, và bạn muốn thống nhất kế hoạch trước khi có code:** Coppice. Xem [Bắt đầu](/docs/getting-started).

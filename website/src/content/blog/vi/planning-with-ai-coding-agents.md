---
title: Cách lên kế hoạch với AI coding agent trước khi viết code
metaTitle: Lên kế hoạch với AI coding agent
description: "Xin một kế hoạch ngắn từ coding agent trước khi nó sửa code: kế hoạch tốt gồm những gì, và ba cách làm, từ plan mode đến bước kế hoạch bạn duyệt."
answer: "Trước khi agent sửa code, hãy xin một kế hoạch ngắn: hướng làm, các bước, và cách kiểm tra thay đổi. Để agent hỏi trước, và so các phương án khi thật sự có lựa chọn. Bạn có thể dùng plan mode của CLI, một file kế hoạch trong repository, hoặc một bảng như Coppice, nơi kế hoạch là một bước riêng bạn duyệt, rồi code được review theo kế hoạch đó."
published: "2026-11-09"
draft: false
author: Coppice
authorType: Organization
keywords:
  - AI coding agent
  - kế hoạch
  - Claude Code
  - Codex
---

## Vì sao nên lên kế hoạch trước

Khi agent viết code ngay, hiểu nhầm chỉ hiện ra trong diff. Lúc đó agent đã làm sai việc, và bạn mất thời gian đọc, giải thích ý mình, rồi chờ lần thử tiếp theo. Một kế hoạch cho thấy cùng một hiểu nhầm trong vài dòng, đọc xong trong một phút.

Xin một kế hoạch thì dễ: bạn yêu cầu, agent nào cũng viết. Khó là làm cho kế hoạch có ích. Yêu cầu mơ hồ, agent đoán thay vì hỏi, kế hoạch chỉ liệt kê bước mà không có quyết định phía sau, và khi đã bắt đầu viết code thì không ai đối chiếu code với kế hoạch.

## Một kế hoạch tốt có gì

- **Kết quả và ràng buộc, bằng lời của bạn.** “Settings có một công tắc dark mode, theo cùng thiết lập theme với phần còn lại của ứng dụng.”
- **Câu trả lời, không phải đoán.** Agent hỏi những gì nó cần biết trước khi lên kế hoạch.
- **Hướng làm, và lý do chọn hướng đó.** Khi thật sự có lựa chọn, chẳng hạn bố cục hoặc data model, agent đưa các phương án và đề xuất một phương án.
- **Vài bước, gồm cả bài test** cho thấy thay đổi chạy được.
- **Những gì nó sẽ không đổi.**

Nếu một hai phút vẫn chưa đọc xong, việc này có thể quá lớn. Hãy tách ra.

## Cách 1: plan mode trong CLI

Claude Code có plan mode: agent đọc code và đề xuất kế hoạch mà không sửa file. Với agent khác, hãy xin một kế hoạch và bảo agent đừng sửa gì cho đến khi bạn duyệt.

Đây là cách bắt đầu nhanh nhất. Giới hạn là kế hoạch chỉ nằm trong cuộc hội thoại. Phiên kết thúc thì kế hoạch mất, và bạn khó đối chiếu diff cuối với kế hoạch đó.

## Cách 2: một file kế hoạch trong repository

Bảo agent viết kế hoạch vào một file Markdown, ví dụ `docs/plans/dark-mode.md`. Bạn tự sửa file, rồi bắt đầu phần làm bằng “Implement docs/plans/dark-mode.md”, trên cùng agent hoặc một agent khác.

Kế hoạch còn lại qua các phiên và các agent, và bạn có thể review nó như code. Bạn vẫn quản lý file, quyết định khi nào kế hoạch được duyệt, và tự so diff với kế hoạch.

## Cách 3: một bước kế hoạch bạn duyệt, với Coppice

[Coppice](/) là ứng dụng desktop đặt các agent lập trình lên một bảng việc. Mọi ticket đều bắt đầu bằng một kế hoạch. Agent PM có thể hỏi bạn tối đa ba câu trước, rồi mới viết kế hoạch. Với thay đổi giao diện, agent còn đưa các phương án thiết kế để bạn so và chọn. Lần chạy lên kế hoạch không sửa code của bạn. Bạn duyệt xong, kế hoạch được lưu trên ticket, các agent làm theo kế hoạch đó, và bạn review diff ngay cạnh kế hoạch.

1. [Cài Coppice](/docs/install) và thêm dự án.
2. Viết một ticket gồm kết quả và ràng buộc, rồi cho agent chạy.
3. Trả lời câu hỏi của agent PM, nếu agent có hỏi.
4. Đọc kế hoạch. Với thay đổi giao diện, mở các phương án thiết kế và chọn một. Rồi duyệt kế hoạch, hoặc yêu cầu sửa.
5. Khi code đã sẵn sàng, review diff theo kế hoạch đã duyệt.

Với ticket yêu cầu một công tắc dark mode trong Settings, kế hoạch nêu hướng làm và ba bước, trong đó có một bài test phần thiết lập:

![Một ticket tiêu đề Add a dark mode toggle to Settings, đang ở Plan Review. Kế hoạch đề xuất làm một công tắc trên dòng Settings và liệt kê ba bước: thêm công tắc Settings, dùng theme hook sẵn có, và thêm một bài test phần thiết lập.](/blog/planning-with-ai-coding-agents/plan.png "Kế hoạch nêu hướng làm và các bước trước khi có code.")

Cùng ticket đó có ba bố cục để so. Agent PM đề xuất một bố cục, và chọn bố cục khác sẽ cập nhật kế hoạch:

![Các phương án thiết kế cho công tắc dark mode: A, một công tắc trên dòng Settings, được đánh dấu Recommended; B, các nút Light, Dark và System, hiện trong bản xem trước trực tiếp; và C, các thẻ xem trước theme.](/blog/planning-with-ai-coding-agents/design-options.png "So các bố cục trước khi agent làm một bố cục.")

[Khái niệm](/docs/concepts) giải thích từng bước một ticket đi qua.

## Chọn cách nào

- **Việc nhỏ trong một phiên:** plan mode.
- **Việc lớn hơn, qua nhiều phiên hoặc nhiều agent:** một file kế hoạch.
- **Nhiều việc, và kế hoạch là một bước bạn duyệt trước khi có code:** Coppice. Xem [Bắt đầu](/docs/getting-started).

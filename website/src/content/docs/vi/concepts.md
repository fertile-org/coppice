---
title: Khái niệm
description: "Ticket đi từ Backlog đến Done ra sao, và ai làm việc gì."
---

# Khái niệm

Coppice là một bảng để quản lý việc lập trình. Bạn viết ticket, agent làm trên branch riêng, và bạn review mọi thay đổi trước khi đưa vào code. Bạn chưa bấm thì chưa có gì chạy, và bạn chưa chọn merge thì chưa có gì được merge.

<a id="how-a-ticket-moves"></a>

## Ticket đi qua các cột thế nào

1. **Backlog.** Bạn tạo ticket, hoặc duyệt ticket mà agent đề xuất.
2. **Plan Review.** Bấm **Run agent**, agent PM sẽ viết kế hoạch. Đọc xong thì duyệt, hoặc để lại bình luận. Nếu ticket còn mơ hồ, agent PM có thể hỏi bạn tối đa 3 câu ngắn trước, và thẻ sẽ hiện "Waiting for your answers". Trả lời ngay trong ticket rồi bấm **Send answers**, agent PM sẽ viết kế hoạch. Khâu lên kế hoạch không bao giờ sửa code của ticket. Nếu bật **Skip planning**, ticket đi thẳng từ Backlog sang Ready. Với luồng xử lý, kiến trúc hay mô hình dữ liệu, kế hoạch có thể kèm một sơ đồ nhỏ, nhưng phần viết bằng chữ thì lúc nào cũng có.
3. **Ready.** Khi bạn duyệt kế hoạch, agent Technical Lead chọn người làm. Nếu không có Technical Lead hoặc không ai phù hợp, ticket nằm lại ở Ready, chưa giao cho ai, kèm một bình luận.
4. **In Progress, In Review, In QA.** Các agent viết code, review và test trên branch riêng của ticket. Bản checkout chính của bạn không bị đụng tới.
5. **Wait for Human Review.** Bạn mở diff, góp ý bằng bình luận, và bấm **Accept** khi đã ổn. Nếu sau đó agent thêm commit, ticket quay lại In Review rồi mới về lại tay bạn.
6. **Done.** Chấp nhận chưa phải là merge. Khi sẵn sàng, chọn **Merge…**, và thứ bạn đã chấp nhận chính là thứ được merge. Nếu có file bị conflict, Coppice để nguyên main và đưa ticket về In Progress để chính agent đó xử lý. Bạn chấp nhận lại một lần nữa rồi mới merge.

Ticket bị kẹt sẽ chuyển sang **Blocked**, kèm bình luận giải thích lý do.

<a id="agents"></a>

## Agent

Coppice chạy các agent CLI bạn đang dùng, như Claude Code, Codex, Cursor, OpenCode, Kilo Code, GitHub Copilot, Qwen Code, Kimi Code và Grok. Mỗi agent có một vai trò, như PM, Technical Lead hay Backend Engineer, và chạy trên một CLI. Xem [Nhà cung cấp](/docs/providers).

Mỗi agent có thể dùng model riêng, kể cả khi chung một CLI. Ví dụ PM chạy Claude Opus, còn Backend Engineer chạy Claude Sonnet.

<a id="chat"></a>

## Chat

Mở **Chat** và bấm vào một agent để bàn về code, một ý tưởng hay một ticket. Agent nào cũng xem được mọi repository, bảng, ticket và Knowledge của bạn. Agent có thể soạn sẵn ticket hoặc gợi ý một ghi chú Knowledge, nhưng phải đợi bạn duyệt mới được lưu. Chọn **New chat** để mở một cuộc chat mới.

CLI nào cũng chat được. Coppice chưa xác nhận được Claude Code, Codex, Cursor hay Kilo Code luôn chỉ đọc mà không sửa gì, nên khi chat với chúng sẽ có banner cảnh báo. Agent vẫn có thể chạy lệnh trên máy bạn.

<a id="everything-is-on-the-card"></a>

## Mọi thứ đều nằm trên thẻ

Bình luận, kế hoạch và các lần chạy agent đều lưu trên ticket. Các agent chỉ nói chuyện với nhau qua bình luận của ticket, nên có chuyện gì xảy ra bạn cũng đọc được ở đó. Agent mở được các file bạn đính kèm vào mô tả hoặc bình luận.

<a id="knowledge"></a>

## Knowledge

Agent có thể ghi chú lại những điều về dự án để dùng cho các ticket sau. Mặc định, ghi chú nào được giữ là do bạn duyệt.

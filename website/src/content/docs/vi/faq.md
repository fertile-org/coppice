---
title: Hỏi đáp
metaTitle: "Hỏi đáp về Coppice: Beta, Docker, Windows và merge"
description: "Coppice đã dùng cho production được chưa? Có cần Docker hay tài khoản không? Agent có tự merge không? Giải đáp về bản Beta, Windows, xung đột và hơn thế."
---

# Hỏi đáp

<a id="is-coppice-production-ready"></a>

## Coppice đã dùng được cho production chưa?
Chưa. Coppice đang ở bản beta, dành cho những ai muốn dùng sớm một bảng quản lý agent chạy ngay trên máy, có các bước duyệt thật sự của con người. Agent chạy với quyền của tài khoản trên máy bạn.

<a id="do-i-need-docker-or-an-account"></a>

## Có cần Docker hay tài khoản không?
Không. Tải ứng dụng desktop về, mở lên là dùng. Không cần Docker, không phải dựng server, không cần tài khoản Coppice.

<a id="is-this-a-jira--linear-replacement"></a>

## Coppice có thay được Jira / Linear không?
Không. Coppice là bảng để điều phối coding agent, không phải hệ thống quản lý issue cho cả công ty.

<a id="is-this-a-cloud-coding-agent"></a>

## Đây có phải coding agent chạy trên đám mây không?
Không. Agent chạy qua các CLI trên máy bạn, làm trên repo của bạn. Khác biệt nằm ở chỗ: dịch vụ agent trên đám mây đẩy việc sang máy của người khác, còn Coppice giữ việc trên máy bạn và mọi thứ phải qua tay bạn duyệt.

<a id="will-agents-merge-without-me"></a>

## Agent có tự merge khi tôi chưa đồng ý không?
Không. Human Review là bước duyệt bắt buộc. Bạn mở diff và tự quyết định.

<a id="what-happens-if-a-tickets-changes-conflict-with-main"></a>

## Nếu thay đổi của ticket bị conflict với main thì sao?
Coppice dừng lại trước khi merge và để nguyên main. Nó liệt kê các file bị conflict rồi đưa ticket về In Progress, để chính agent đó xử lý. Sau đó bạn chấp nhận lại ở Human Review rồi mới merge.

<a id="what-about-set-and-forget-autopilots"></a>

## Còn kiểu “giao xong là khỏi lo”, để agent tự lái hoàn toàn thì sao?
Coppice không làm theo hướng đó. Coppice sinh ra để bạn quản lý agent: bạn là người bắt đầu công việc, và bạn duyệt từng thay đổi.

<a id="windows"></a>

## Còn Windows thì sao?
Windows 11 có thể chạy bản Linux x64 qua WSL, nhưng chúng tôi chưa thử. Xem [Windows 11 (qua WSL)](/docs/install#windows-11-through-wsl). Nếu bạn thử, hãy [kể lại cho chúng tôi trên GitHub](https://github.com/fertile-org/coppice/issues). Bản beta này chưa có kế hoạch làm ứng dụng Windows riêng.

<a id="where-do-contributors-look"></a>

## Người đóng góp tìm tài liệu ở đâu?
Tài liệu về kiến trúc, cách phát triển và kiểm thử nằm trong [repo GitHub](https://github.com/fertile-org/coppice/tree/main/docs). Trang này dành cho người dùng Coppice.

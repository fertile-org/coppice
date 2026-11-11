---
title: Cách dùng Fable để lên kế hoạch và viết code bằng model rẻ hơn
metaTitle: Lên kế hoạch bằng Fable, code bằng model rẻ hơn
description: "Lên kế hoạch bằng Fable 5.2 và viết code bằng Opus hoặc Sonnet. Claude Code không có chế độ Fable rồi model rẻ hơn. Coppice có thể đặt một model cho mỗi agent."
answer: "Dùng Fable 5.2 cho kế hoạch và một model rẻ hơn, Opus hoặc Sonnet, cho phần code. Claude Code không có chế độ này: opusplan vẫn là Opus rồi Sonnet. Lên kế hoạch bằng /model fable rồi đổi model, hoặc giao model riêng cho subagent. Trong Coppice, mỗi agent có model riêng, nên PM có thể lên kế hoạch bằng Fable và kỹ sư viết code bằng Opus hoặc Sonnet."
published: "2026-11-11"
draft: false
author: Coppice
authorType: Organization
keywords:
  - Fable
  - Opus
  - Sonnet
  - Claude Code
  - kế hoạch
---

## Vì sao việc này khó

Claude Fable 5.2 là model của Anthropic cho việc khó nhất và còn để ngỏ. Với phần lớn việc viết code, Anthropic nói hãy bắt đầu bằng Opus 5.5, và dùng Sonnet 5.5 cho việc nhỏ hơn, phạm vi rõ hơn. Fable đắt hơn Opus, và Opus đắt hơn Sonnet. Phần lớn token phát sinh sau kế hoạch: đọc file, sửa, rồi thử lại. Để Fable chạy tiếp đoạn đó thì Fable làm cả việc máy móc. Bạn muốn Fable lo kế hoạch, Opus hoặc Sonnet lo code, và kế hoạch vẫn còn khi bắt đầu viết code.

## Cách 1: lên kế hoạch bằng Fable, rồi đổi model

Claude Code không có chế độ nào dùng Fable cho kế hoạch và model rẻ hơn cho code. `opusplan` vẫn là Opus ở plan mode, rồi Sonnet sau đó.

Chạy `/model fable`, rồi vào plan mode. Trong terminal, alias này là Fable 5.2. Trong một phiên Claude apps gateway, alias này vẫn là Fable 5, nên ở đó hãy chạy `/model claude-fable-5-2`. Khi kế hoạch xong, chạy `/model opus` hoặc `/model sonnet`. Cuộc hội thoại vẫn còn. Khi phiên kết thúc, kế hoạch mất.

## Cách 2: subagent có model riêng

Subagent của Claude Code là một file Markdown. Trường `model` đặt model của subagent đó: `fable`, `opus`, `sonnet`, `haiku`, hoặc một id đầy đủ. Giữ phiên trên Fable và giao phần code cho Opus hoặc Sonnet. Lưu file này trong `.claude/agents/`:

```markdown
---
name: implementer
description: Implements an approved plan
model: opus
---
```

Dùng `sonnet` khi việc nhỏ. Chỉ phần được giao mới chạy trên model đó. Code mà phiên chính tự viết vẫn chạy trên Fable.

## Cách 3: dán kế hoạch vào một phiên rẻ hơn

Lên kế hoạch bằng Fable, chép kế hoạch, rồi bắt đầu lại trên Opus hoặc Sonnet:

```bash
claude --model opus
```

`claude --model sonnet` là cùng một bước cho việc nhỏ hơn. Dán kế hoạch vào. Bạn cũng có thể dán như vậy vào một agent CLI khác. Phiên mới chỉ biết những gì bạn đã dán.

## Cách 4: mỗi agent một model, với Coppice

[Coppice](/) là ứng dụng desktop đặt các agent lập trình lên một bảng việc. Mỗi agent có model riêng, kể cả khi cùng một CLI. Với ticket như công tắc dark mode trong Settings, PM có thể lên kế hoạch bằng Fable 5.2 và Frontend Engineer có thể viết bằng Opus 5.5 hoặc Sonnet.

1. [Cài Coppice](/docs/install). Đăng nhập Claude Code trong terminal như bạn vẫn làm, rồi thêm dự án.
2. Tạo agent PM, chọn Claude Code, và đặt model là Fable 5.2.
3. Tạo Technical Lead và Frontend Engineer trên Claude Code. Đặt model của Frontend Engineer là Opus 5.5, hoặc Sonnet khi thay đổi nhỏ.
4. Viết ticket và cho agent chạy. Agent PM viết kế hoạch bằng Fable. Đọc xong thì duyệt kế hoạch, hoặc yêu cầu sửa. Lần chạy lên kế hoạch này không sửa code của bạn.
5. Khi code đã sẵn sàng, review diff.

Sau khi bạn duyệt, Technical Lead kiểm tra kế hoạch và giao cho Frontend Engineer. Lần chạy của Frontend Engineer dùng model bạn đã đặt, và tự bắt đầu. [Khái niệm](/docs/concepts) giải thích từng bước.

![Hộp tạo agent mới tên PM. Danh sách model đang mở, và Claude Fable 5.2 được chọn.](/blog/fable-for-planning/pm-fable.png "Danh sách model của PM, với Claude Fable 5.2 được chọn.")

![Hộp tạo agent mới cho Frontend Engineer. Claude Code được chọn và ghi Ready, model là Claude Opus 5.5.](/blog/fable-for-planning/frontend-opus.png "Frontend Engineer dùng Claude Code, với Claude Opus 5.5 được chọn.")

## Chọn cách nào

- **Một phiên Claude Code, và kế hoạch phải nằm trên Fable:** `/model fable`, rồi `/model opus` hoặc `/model sonnet`. Dùng `opusplan` khi Opus và Sonnet là đủ.
- **Model rẻ hơn cho một bước bạn giao đi:** một subagent với `model: opus` hoặc `model: sonnet`.
- **Một kế hoạch muốn đưa sang phiên mới:** dán vào phiên chạy Opus hoặc Sonnet.
- **Nhiều ticket, người lên kế hoạch dùng Fable và người viết code dùng Opus hoặc Sonnet:** Coppice. Xem [Bắt đầu](/docs/getting-started).

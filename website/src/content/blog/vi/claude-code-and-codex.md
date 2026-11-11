---
title: Cách dùng Claude Code và Codex trên cùng một dự án
metaTitle: Claude Code và Codex trên một dự án
description: "Ba cách chạy Claude Code và Codex trên cùng một bộ code: hai terminal, git worktree, hoặc một bảng để mỗi agent nhận ticket riêng."
answer: "Cho mỗi công cụ một bản code riêng để chúng không sửa cùng một file vào cùng lúc. Bạn có thể tự tách bằng git worktree, hoặc dùng một bảng như Coppice, nơi mỗi ticket có branch riêng và bạn chọn Claude Code hay Codex làm ticket đó. Dù cách nào, bạn review thay đổi trước khi chúng vào branch chính."
published: "2026-11-07"
updated: "2026-11-08"
draft: false
author: Coppice
authorType: Organization
keywords:
  - Claude Code
  - Codex
  - agent lập trình
  - git worktree
---

## Vì sao dùng cả hai

Claude Code và Codex đều là agent lập trình chạy trong terminal. Bạn có thể để một bên lên kế hoạch và bên kia viết code, hoặc để model thứ hai review phần model thứ nhất đã sửa. Khó không phải là chạy chúng. Khó là tách các thay đổi để biết ai sửa gì.

Có ba cách thường gặp.

## Cách 1: hai terminal, một checkout

Mở dự án trong hai terminal. Chạy `claude` ở một bên và `codex` ở bên kia.

Đây là cách bắt đầu nhanh nhất. Cách này hợp khi bạn chỉ dùng một công cụ mỗi lần, ví dụ dùng Codex để review phần Claude Code vừa viết. Nếu cả hai cùng sửa, chúng có thể đụng cùng một file, và bên viết sau có thể ghi đè phần của bên kia.

## Cách 2: mỗi công cụ một git worktree

Git worktree là thư mục làm việc thứ hai của cùng một repository, trên branch riêng. Tạo một worktree cho Codex:

```bash
git worktree add ../my-project-codex -b codex-task
```

Chạy `claude` trong thư mục chính và `codex` trong `../my-project-codex`. Cả hai có thể làm cùng lúc mà không đụng file của nhau. Xong thì bạn tự review từng branch, rồi tự merge.

Việc được tách ra, nhưng phần điều phối vẫn là của bạn: bạn quyết định việc nào giao cho công cụ nào, chuyển ngữ cảnh giữa hai bên, và dọn các branch.

## Cách 3: một bảng cho cả hai, với Coppice

[Coppice](/) là ứng dụng desktop đặt các agent lập trình lên một bảng việc. Coppice chạy các lệnh `claude` và `codex` đã có trên máy bạn. Mỗi ticket chạy trên branch riêng, nên bản checkout chính không bị thay đổi, và các agent chuyển ticket cho nhau qua bảng.

1. [Cài Coppice](/docs/install). Đăng nhập Claude Code và Codex trong terminal như bạn vẫn làm.
2. Thêm thư mục dự án.
3. Tạo agent, và chọn Claude Code hoặc Codex cho từng agent. Ví dụ PM và Technical Lead dùng Claude Code, Backend Engineer dùng Codex, Reviewer dùng Claude Code, để Claude Code review phần Codex viết.
4. Viết ticket và cho agent chạy.

PM viết kế hoạch, và bạn duyệt. Sau đó các agent viết và review thay đổi. Bạn đọc diff cuối, chấp nhận, rồi merge khi đã sẵn sàng.

![Form tạo agent mới. Preset PM được chọn, và CLI của agent là Claude Code.](/blog/claude-code-and-codex/new-agent.png "Mỗi agent chạy trên một CLI. Chọn Claude Code hoặc Codex khi tạo agent.")

## Chọn cách nào

- **Việc nhanh, mỗi lần một công cụ:** hai terminal.
- **Làm song song, và bạn tự quản lý branch:** git worktree.
- **Nhiều ticket, agent chuyển việc và review lẫn nhau, còn bạn duyệt kế hoạch và diff:** Coppice. Xem [Bắt đầu](/docs/getting-started).

---
title: Cách chạy nhiều agent lập trình song song bằng Git worktree
metaTitle: Agent song song với git worktree
description: "Chạy vài agent lập trình cùng lúc mà không để chúng ghi đè nhau: mỗi việc một git worktree, một script nhỏ, hoặc một bảng tự chuẩn bị giúp bạn."
answer: "Cho mỗi agent một git worktree riêng: một thư mục riêng của cùng repository, trên branch riêng, để chúng không sửa cùng các file. Bạn có thể tạo bằng tay với git worktree add, gói vào một script nhỏ, hoặc dùng bảng như Coppice. Bảng đó tạo branch và worktree cho mỗi ticket, và hiện mọi lần chạy ở một chỗ. Dù cách nào, bạn review rồi merge từng branch."
published: "2026-11-07"
updated: "2026-11-09"
author: Coppice
authorType: Organization
draft: false
keywords:
  - git worktree
  - agent song song
  - agent lập trình
  - Claude Code
---

## Vì sao việc này khó

Hai agent trong cùng một thư mục sửa cùng các file, và một bên làm hỏng việc của bên kia. Git worktree xử lý chuyện đó: mỗi agent có thư mục và branch riêng.

Phần khó sau đó chuyển chỗ. Mỗi agent một terminal, bạn phải nhớ cái nào đã xong và cái nào đang chờ bạn, rồi merge từng branch. Hai việc đụng cùng các file vẫn có thể xung đột lúc merge.

## Cách 1: tự tạo worktree

Mỗi việc tạo một worktree:

```bash
git worktree add ../my-project-login -b login
git worktree add ../my-project-search -b search
```

Mở một terminal trong mỗi thư mục và chạy agent ở đó. Khi việc xong, review và merge branch của nó, rồi xóa thư mục bằng `git worktree remove`.

Cách này hợp với hai hoặc ba việc. Nhiều hơn thì việc chuyển terminal và theo dõi từng việc trở thành công việc chính.

## Cách 2: gói vào một script nhỏ

Một hàm shell có thể tạo worktree và chạy agent trong một bước:

```bash
agent_task() {
  dir="../$(basename "$PWD")-$1"
  git worktree add "$dir" -b "$1" && cd "$dir" && claude
}
```

Chạy `agent_task login` từ thư mục dự án. Đổi `claude` thành `codex`, hoặc CLI của agent khác.

Cách này bớt phần gõ lúc chuẩn bị. Mỗi việc vẫn là một terminal, và theo dõi cùng việc merge vẫn do bạn làm.

## Cách 3: một bảng cho mọi việc, với Coppice

[Coppice](/) là ứng dụng desktop đặt các agent lập trình lên một bảng việc. Khi việc bắt đầu, mỗi ticket có branch và worktree riêng, nên bản checkout chính không bị thay đổi. Nhiều ticket có thể chạy cùng lúc, và bảng cho thấy mỗi ticket đang ở bước nào.

1. [Cài Coppice](/docs/install) và thêm dự án.
2. Tạo các agent, rồi chọn Claude Code, Codex, hoặc CLI khác cho từng agent.
3. Mỗi việc viết một ticket, rồi cho agent chạy.
4. Duyệt từng kế hoạch, rồi review từng diff khi đã sẵn sàng.
5. Merge từng ticket một. Nếu merge bị xung đột, Coppice liệt kê các file, và bạn có thể nhờ một agent xử lý.

![Phần Git actions của một ticket, hiện branch riêng agent/TICKET-723e8f0a và thư mục worktree.](/blog/parallel-agents-worktrees/branch-and-worktree.png "Mỗi ticket có branch và worktree riêng.")

[Khái niệm](/docs/concepts) giải thích từng bước một ticket đi qua.

## Tách việc để merge gọn

Dù dùng cách nào:

- Mỗi việc dùng file riêng. File dùng chung như lockfile, schema, hoặc một module dùng chung thuộc về một việc, hoặc về việc sau.
- Merge một branch, cập nhật branch kế tiếp lên kết quả đó, rồi kiểm tra hai thay đổi đặt cạnh nhau vẫn còn hợp lý. Git có thể merge hai file gọn dù nội dung mâu thuẫn nhau.

## Chọn cách nào

- **Hai hoặc ba việc:** tự tạo worktree.
- **Bạn hay bắt đầu việc mới và thích terminal:** một script nhỏ.
- **Nhiều việc cùng lúc, và bạn muốn một chỗ để xem, review, và merge:** Coppice. Xem [Bắt đầu](/docs/getting-started).

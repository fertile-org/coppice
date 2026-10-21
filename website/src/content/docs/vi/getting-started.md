---
title: Bắt đầu
description: "Năm bước từ lúc cài xong đến lần chạy agent đầu tiên."
---

# Bắt đầu

Chỉ cần năm bước là đi từ lúc cài xong đến lần chạy agent đầu tiên. Bạn cần một git checkout trên máy và một agent CLI. Không cần tài khoản Coppice.

Ở lần cài đầu tiên, Coppice mở một màn hình thiết lập ngắn để thêm repository, bảng và đội agent khởi đầu. Muốn mở lại, vào **System → Connectors** và bấm **Set up workspace**.

<a id="1-install-a-provider-and-sign-in"></a>

## 1. Cài nhà cung cấp và đăng nhập
Nhà cung cấp là agent CLI mà Coppice chạy giúp bạn: Claude Code, Codex, Cursor, OpenCode, Kilo Code, GitHub Copilot, Qwen Code, Kimi Code hoặc Grok. Coppice không tự cài các CLI này mà tìm những CLI đã có sẵn trên máy.

1. Cài CLI và đăng nhập trong terminal, như bạn vẫn làm.
2. Trong Coppice, mở **System** ở thanh bên, rồi chọn **Connectors**.
3. Tìm thẻ của CLI đó. Thẻ hiện **Ready** là Coppice dùng được.

Nếu thẻ hiện **Found, not signed in**, hãy đăng nhập CLI trong terminal. Nếu hiện **Not on your PATH**, hãy cài CLI, hoặc làm theo **Install docs** trên thẻ. Xong thì bấm **Run check**.

Kilo Code, GitHub Copilot, Qwen Code và Kimi Code hiện **Installed**. Coppice không kiểm tra được trạng thái đăng nhập của các CLI này nếu không dùng đến gói của bạn, nên nếu bạn đã đăng xuất thì lần chạy sẽ báo lỗi.

<a id="2-set-up-an-agent"></a>

## 2. Tạo agent
1. Mở **Agents** ở thanh bên và bấm **New agent**.
2. Chọn một **Preset**. Preset điền sẵn vai trò và prompt.
3. Nhập **Name**.
4. Chọn **Connector**. Coppice chọn sẵn connector đầu tiên đã sẵn sàng.
5. Chọn **Model**, hoặc để mặc định.
6. Bấm **Create agent**.

Lưu agent xong, connector đó sẽ được bật. Bạn sẽ thấy dòng "{name} turned on."

Hãy thêm một agent **PM**, một agent **Technical Lead**, và ít nhất một agent làm việc (**Frontend Engineer**, **Backend Engineer** hoặc **Research**). Khâu lên kế hoạch và chuyển giao việc cần đủ cả ba.

<figure>
<img src="/assets/docs/new-agent.webp" width="1440" height="900" alt="Form tạo agent mới, đã điền tên, vai trò và chọn một CLI.">
<figcaption>Mỗi agent chạy trên một CLI của bạn.</figcaption>
</figure>

<a id="3-add-a-repository"></a>

## 3. Thêm repository
1. Mở **Repositories** ở thanh bên và bấm **Add repository**.
2. Cạnh **Local path**, bấm **Browse…** và chọn một git checkout trên máy.
3. Coppice sẽ cố điền sẵn **Name**, **Remote URL (optional)** và **Default branch** từ checkout đó. Hãy kiểm tra lại.
4. Bấm **Add repository**.

Dòng mới sẽ hiện **Ready**. Nếu hiện **Path missing** hoặc **Not a git repo**, hãy chọn lại thư mục.

<a id="4-set-up-a-board"></a>

## 4. Tạo bảng
1. Mở **Boards** ở thanh bên.
2. Bấm **New board**. Nếu vừa cài lần đầu, bấm **Create board**.
3. Nhập **Name** rồi bấm **Create board**.

Coppice sẽ mở bảng vừa tạo.

<a id="5-create-a-ticket-and-run-it"></a>

## 5. Tạo ticket và chạy
1. Ở cột **Backlog**, gõ tiêu đề vào ô **Add ticket…** và nhấn Enter.
2. Mở thẻ vừa tạo.
3. Bấm **Edit** để thêm mô tả, rồi bấm **Save**.
4. Ở tab **Detail**, chọn **Assignee** là agent của bạn và **Repository** là repository vừa thêm. Bấm **Save metadata**.
5. Bấm **Run agent**.

Ticket chuyển sang tab **Live Console** để bạn xem agent làm việc. Bấm **Stop** để dừng lần chạy.

Nút **Run agent** chỉ bật khi ticket đã có người được giao và repository đang ở trạng thái **Ready**. Chỉ giao agent thôi thì chưa tự chạy.

<figure>
<img src="/assets/docs/backlog-ticket.webp" width="1440" height="900" alt="Một bảng có ticket mới ở cột Backlog, nút Run agent đang hiện.">
<figcaption>Bấm Run agent là PM bắt đầu viết kế hoạch.</figcaption>
</figure>

<a id="what-happens-next"></a>

## Sau đó thì sao
1. **Plan Review.** Agent PM viết kế hoạch, và ticket chuyển sang Plan Review. Kế hoạch hiện ngay dưới phần mô tả, bên dưới là **Approve plan** và **Ask for changes**. Khi duyệt, kế hoạch được thêm vào mô tả dưới mục **Approved plan**, sau phần bạn viết.
2. **Ready.** Bạn duyệt xong, ticket chuyển sang Ready. Agent Technical Lead đối chiếu kế hoạch với code, chỉ sửa kế hoạch đã duyệt khi cần, rồi chọn người làm. Agent đó bắt đầu chạy ngay. Nếu không ai phù hợp, ticket sẽ để trống người được giao kèm một bình luận giải thích.
3. **In Progress.** Agent được giao làm việc trong git worktree riêng.
4. **Wait for Human Review.** Bạn đọc diff, rồi bấm **Accept** trong Human Review.
5. **Done.** Thẻ ở cột Done có thể hiện "Accepted, not merged". Mở ticket và chọn **Merge…**, hoặc **Create PR via API** nếu bạn muốn review qua pull request.

Muốn bỏ qua bước lên kế hoạch, đánh dấu **Skip planning** trên ticket. Ticket sẽ đi thẳng từ Backlog sang Ready.

---
title: Nhà cung cấp
description: "Các agent CLI mà Coppice chạy: Claude Code, Codex, Cursor, OpenCode, Kilo Code, GitHub Copilot, Qwen Code, Kimi Code và Grok."
---

# Nhà cung cấp

Ngán cảnh mỗi agent phải mở một terminal? Coppice chạy luôn các coding agent CLI bạn đang dùng và gom việc của chúng lên một bảng. Bạn vẫn giữ gói đăng ký và tài khoản của mình. Không cần tài khoản Coppice.

<a id="supported-connectors"></a>

## Các connector được hỗ trợ

| Connector | Lệnh Coppice tìm | Cài đặt |
| --- | --- | --- |
| Claude Code | `claude` | [Hướng dẫn cài](https://docs.anthropic.com/en/docs/claude-code/setup) |
| Codex | `codex` | [Hướng dẫn cài](https://developers.openai.com/codex/cli) |
| Cursor | `cursor-agent` | [Hướng dẫn cài](https://cursor.com/docs/cli/installation) |
| OpenCode | `opencode` | [Hướng dẫn cài](https://opencode.ai/docs/) |
| Kilo Code | `kilo` | [Hướng dẫn cài](https://kilo.ai/docs/cli) |
| GitHub Copilot | `copilot` | [Hướng dẫn cài](https://docs.github.com/en/copilot/how-tos/copilot-cli) |
| Qwen Code | `qwen` | [Hướng dẫn cài](https://github.com/QwenLM/qwen-code) |
| Kimi Code | `kimi` | [Hướng dẫn cài](https://moonshotai.github.io/kimi-code/) |
| Grok | `grok` | [Hướng dẫn cài](https://x.ai/cli) |

<figure>
<img src="/assets/docs/model-picker.webp" width="1440" height="900" alt="Ô chọn model của một agent, liệt kê các model lấy từ CLI của agent đó.">
<figcaption>Chọn một model, hoặc để mặc định.</figcaption>
</figure>

Chọn model cho agent này, hoặc để mặc định. Mới cài thêm model? Bấm Refresh models.

<a id="connect-your-agent-cli"></a>

## Kết nối agent CLI

1. Cài CLI và đăng nhập trong terminal, như bạn vẫn làm.
2. Khi tạo agent, chọn agent CLI muốn dùng, Coppice sẽ bật nó lên.

Form tạo agent liệt kê mọi connector kèm trạng thái:

| Trạng thái | Nghĩa là |
| --- | --- |
| **Ready** | Coppice đã kiểm tra (không tốn phí) và thấy bạn đang đăng nhập. |
| **Found, not signed in** | Coppice tìm thấy lệnh nhưng bạn chưa đăng nhập. Hãy đăng nhập trong terminal. |
| **Not on your PATH** | Coppice không tìm thấy lệnh. Hãy cài CLI, hoặc kiểm tra lại PATH. |
| **Installed** | Coppice tìm thấy lệnh. Với Kilo Code, GitHub Copilot, Qwen Code, Kimi Code và Grok, Coppice không kiểm tra được trạng thái đăng nhập nếu không dùng đến gói của bạn. |

Kilo Code, GitHub Copilot, Qwen Code, Kimi Code và Grok hiện **Installed**. Trong System → Connectors có thêm dòng: "Coppice can't check this sign-in without using your plan. A run will show an error if you're signed out." Muốn chắc chắn là chạy được, dùng **Test connection**: nó chạy thử một lượt chat, một lượt lên kế hoạch và một lượt làm ticket ngắn. CLI nào cũng chat được. Khi chat với Claude Code, Codex, Cursor và Kilo Code sẽ có dòng "Coppice hasn't confirmed that {cli} stays read-only. The agent can still run commands on your computer." (Coppice chưa xác nhận được {cli} luôn chỉ đọc. Agent vẫn có thể chạy lệnh trên máy bạn.) Chat với OpenCode, GitHub Copilot, Qwen Code, Kimi Code và Grok thì không có banner này: khi chat và lên kế hoạch, Coppice chặn chúng ghi file và chạy lệnh shell. Riêng các công cụ MCP bạn tự cấu hình trong CLI đó thì vẫn có thể thay đổi dữ liệu.

Bạn có thể lưu agent trước khi CLI sẵn sàng. Khi CLI đã cài và đăng nhập xong, agent sẽ bắt đầu nhận ticket. Nếu bạn cài CLI sau khi đã mở Coppice, hãy thoát rồi mở lại Coppice để nó tìm thấy lệnh mới.

Muốn tắt một connector, vào **System → Connectors**.

<a id="use-your-own-api-key-or-another-provider"></a>

## Dùng API key riêng hoặc nhà cung cấp khác

Muốn dùng API key của riêng mình, hoặc một model mà CLI của bạn không có? Hãy dùng OpenCode. OpenCode hỗ trợ hơn 75 nhà cung cấp model, kể cả model chạy trên máy. Xem [danh sách nhà cung cấp của OpenCode](https://opencode.ai/docs/providers/).

1. Cài OpenCode.
2. Chạy `opencode` trong terminal, gõ `/connect`, chọn nhà cung cấp và dán key vào. Key do OpenCode giữ, Coppice không bao giờ hỏi đến. Không thấy nhà cung cấp của bạn trong danh sách? Làm theo [hướng dẫn thêm nhà cung cấp tùy chỉnh](https://opencode.ai/docs/providers/#custom-provider) của OpenCode.
3. Trong Coppice, tạo agent và chọn **Connector** là **OpenCode**. Các model của nhà cung cấp đó sẽ hiện trong **Model**. Nếu bạn thêm nhà cung cấp trong lúc Coppice đang mở, bấm **Refresh models**.

<a id="on-a-ticket"></a>

## Khi làm một ticket

Khi agent nhận ticket, Coppice chạy CLI của agent đó trong git worktree riêng của ticket. Bạn xem được tiến độ trực tiếp ngay trên ticket.

<a id="what-coppice-does-not-do"></a>

## Những việc Coppice không làm
- Không chạy dịch vụ viết code trên đám mây
- Không merge code khi bạn chưa qua bước Human Review

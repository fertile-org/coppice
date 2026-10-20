---
title: Plugin
description: "Plugin bổ sung skill và công cụ MCP cho agent. Thêm plugin, bật lên, rồi chọn agent nào được dùng."
---

# Plugin

Plugin bổ sung skill và công cụ MCP cho agent. Thêm plugin, bật lên, rồi chọn agent nào được dùng.

Mỗi plugin là một thư mục, có thể chứa skill (các file `SKILL.md`) và công cụ MCP (`.mcp.json`). Mở **Plugins** ở thanh bên để quản lý.

Plugin `coppice` có sẵn và luôn ở đó. Bạn không tắt được các skill của nó.

<a id="install-a-plugin"></a>

## Cài plugin
Dùng **Add plugins**. Có hai cách.

<a id="from-git"></a>

### Từ git
1. Dán **Git URL**.
2. Nếu cần, điền **Ref (optional)**, ví dụ `main`.
3. Chọn **Target directory**.
4. Bấm **Install**.

Coppice clone repo bằng thông tin đăng nhập git trên máy bạn. Sau này bấm **Update** trên thẻ để kéo thay đổi mới về.

<a id="from-a-folder"></a>

### Từ một thư mục
1. Trong **Plugin directories**, bấm **Browse…** và chọn thư mục, hoặc gõ đường dẫn vào **Directory path**.
2. Bấm **Add directory**.

Thư mục này có thể là một plugin, hoặc chứa nhiều plugin trong các thư mục con. Sửa file xong thì bấm **Rescan**.

Nếu hai thư mục có plugin trùng tên, plugin trong thư mục đứng trên sẽ được dùng. Dùng mũi tên để đổi thứ tự. Thư mục mặc định có nhãn **Default** và không xóa được.

<a id="enable-it"></a>

## Bật plugin
Plugin mới thêm vào đều đang tắt.

1. Bật công tắc trên thẻ plugin. Chỉ plugin ở trạng thái `ok` mới bật được.
2. Nếu thẻ có mục **Settings**, điền từng giá trị rồi bấm **Save settings**. Các giá trị này được mã hóa khi lưu.
3. Bấm **Test** để khởi động công cụ của plugin và xem danh sách công cụ. Chưa bật plugin vẫn bấm được.

Plugin có MCP server chạy cục bộ sẽ chạy trên máy bạn với quyền của chính máy bạn, nên Coppice sẽ hỏi bạn xác nhận trước.

Mỗi skill có công tắc riêng. Skill nào bị tắt sẽ ẩn với mọi agent, kể cả các lần chạy đang diễn ra.

<a id="choose-which-agents-can-use-it"></a>

## Chọn agent nào được dùng
Trên thẻ, ở mục **Which agents can use it?**, chọn:

- **All agents**: mọi agent, kể cả agent tạo sau này.
- **Choose agents**: chỉ những agent bạn đánh dấu.

Bạn cũng chọn được plugin ngay trong form agent, ở mục **Skills**. Hai chỗ này luôn khớp với nhau. Thay đổi áp dụng từ lần chạy kế tiếp của mỗi agent.

Plugin mới thêm có thiết lập ban đầu như sau:

- Chỉ có skill: **All agents**.
- Có MCP server: **Choose agents**, chưa đánh dấu agent nào.

<a id="use-it-on-a-ticket"></a>

## Dùng plugin khi làm ticket
Bật plugin và cho agent được giao ticket quyền dùng nó. Sau đó bấm **Run agent**.

Agent thấy tên và mô tả của từng skill, và chỉ nạp skill khi cần. Công cụ có tên dạng `<plugin>__<tool>`.

Muốn xem đã dùng những gì, mở tab **Agent Runs** của ticket, mở rộng một lần chạy, rồi mở **Tools & Skills**. Các lần gọi tới plugin có nhãn **Plugin**.

<a id="create-a-plugin"></a>

## Tự viết plugin
Plugin đơn giản nhất là một thư mục có file `SKILL.md`. Muốn có tên, phiên bản và công cụ MCP, hãy thêm file manifest.

```
hello-coppice/
├── .claude-plugin/
│   └── plugin.json
├── skills/
│   └── greeting-style/
│       └── SKILL.md
└── .mcp.json
```

### `plugin.json`
Đặt ở `.claude-plugin/plugin.json`.

```json
{
  "name": "hello-coppice",
  "version": "0.1.0",
  "description": "Example plugin: one skill and one MCP tool that greets people.",
  "author": { "name": "Coppice" }
}
```

| Trường | Bắt buộc | Ghi chú |
| --- | --- | --- |
| `name` | Có | Gồm chữ cái, chữ số, `.`, `_` và `-`, tối đa 64 ký tự, bắt đầu bằng chữ cái hoặc chữ số. Tên `coppice` đã được dành riêng. |
| `version` | Không | Mặc định là `0.0.0`. |
| `description` | Không | Mặc định để trống. |
| `author` | Không | Một chuỗi, hoặc `{ "name": "..." }`. |
| `skills` | Không | Đường dẫn tới một thư mục skill, hoặc danh sách các thư mục, tính từ gốc plugin. |

Không có `plugin.json` thì thư mục vẫn được coi là plugin nếu có `SKILL.md` ngay ở cấp ngoài cùng, hoặc có skill trong `skills/`, `.agents/skills/`, `.claude/skills/` hoặc `<name>/SKILL.md`. Khi đó plugin lấy tên thư mục và phiên bản `0.0.0`.

<a id="skills"></a>

### Skill
Mỗi skill là một thư mục có file `SKILL.md`, tên thư mục chính là tên skill. File mở đầu bằng frontmatter:

```yaml
---
name: greeting-style
description: How to greet people in ticket comments. Load before writing a greeting.
---
```

`name` và `description` là bắt buộc. Hãy viết mô tả sao cho agent biết lúc nào nên nạp skill.

<a id="mcp-tools"></a>

### Công cụ MCP
Thêm file `.mcp.json` có map `mcpServers`. Mỗi server chạy cục bộ hoặc từ xa.

- Cục bộ: `command`, thêm `args` và `env` nếu cần.
- Từ xa: `"type": "http"`, `url`, thêm `headers` nếu cần.

```json
{
  "mcpServers": {
    "hello": {
      "command": "node",
      "args": ["${CLAUDE_PLUGIN_ROOT}/server/index.mjs"],
      "env": { "HELLO_GREETING": "${HELLO_GREETING:-Hello}" }
    }
  }
}
```

`${NAME}` sẽ thành một ô thiết lập trên thẻ plugin, còn `${NAME:-default}` có thêm giá trị mặc định. Coppice tự điền `${CLAUDE_PLUGIN_ROOT}` bằng thư mục cài plugin, nên biến này không hiện trên thẻ. Chưa hỗ trợ server SSE.

Ví dụ này cung cấp công cụ `hello-coppice__greet` và skill `hello-coppice:greeting-style`. Xem ví dụ đầy đủ trong [`examples/plugins`](https://github.com/fertile-org/coppice/tree/main/examples/plugins).

Các thư mục `commands/`, `agents/` và `hooks/` chưa được hỗ trợ. Chúng hiện "Not supported yet" và không chạy.

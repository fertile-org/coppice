---
title: Plugin
metaTitle: "Plugin: skill và công cụ MCP cho agent"
description: "Plugin bổ sung skill và công cụ MCP cho agent trong Coppice. Cài từ git hoặc thư mục, bật lên, chọn agent nào được dùng, hoặc tự viết plugin."
---

# Plugin

Plugin bổ sung skill và công cụ MCP cho agent. Mỗi plugin là một thư mục, có thể chứa:

- **Skill**: các file `SKILL.md`.
- **Công cụ MCP**: một file `.mcp.json`.

Mở **Plugins** ở thanh bên để quản lý. Plugin `coppice` có sẵn và luôn ở đó, bạn không tắt được các skill của nó.

<a id="install-a-plugin"></a>

## 1. Cài plugin
Trang **Plugins** có ba tab:

- **Installed**: mọi plugin bạn có, dù đến từ đâu.
- **Marketplace**: cách nhanh nhất để thêm plugin.
- **Custom**: cài từ git hoặc từ thư mục.

<a id="from-the-marketplace"></a>

### Từ marketplace
1. Mở **Plugins** và chuyển sang tab **Marketplace**.
2. Tìm kiếm, hoặc lọc theo nguồn hay danh mục.
3. Bấm **Install** trên một mục.

<figure>
<img src="/assets/docs/plugin-marketplace.webp" width="1440" height="900" alt="Tab Marketplace trên trang Plugins, có ô tìm kiếm, bộ lọc theo nguồn và các thẻ plugin với nút Install.">
<figcaption>Tìm trong hàng nghìn plugin và cài chỉ với một cú nhấp.</figcaption>
</figure>

Plugin sẽ xuất hiện ở tab **Installed**, ở trạng thái tắt. Tiếp theo, hãy [bật plugin](#enable-it). Plugin dùng được cho mọi agent bạn cấp quyền, dù agent chạy trên CLI nào.

<a id="marketplace-sources"></a>

#### Nguồn
Có sẵn năm nguồn:

- **coppice**: danh mục đi kèm Coppice.
- **claude-plugins-official**: danh mục chính thức của Anthropic.
- **claude-community**: danh mục cộng đồng của Anthropic.
- **superpowers-marketplace**
- **ponytail**

Cách các nguồn được tải:

- Mỗi nguồn tự tải khi bạn mở tab **Marketplace** lần đầu.
- Nguồn được tải lại ở chế độ nền nếu đã cũ hơn 24 giờ.
- Nguồn tải lỗi sẽ báo lỗi trên chip của nó. Bấm vào chip để thử lại.

<a id="community-plugins"></a>

#### Ai đã xem xét plugin
- Thẻ có nhãn **Coppice** đến từ danh mục của Coppice.
- Mọi thẻ khác có nhãn **Community: not reviewed by Coppice**. Hãy xem xét trước khi bật.
- Mục trong danh mục có ghim `sha` sẽ cài đúng commit đó, kể cả khi nhánh đã thay đổi.

<a id="manage-sources"></a>

#### Quản lý nguồn
Bấm **Manage sources** để xem từng nguồn và lần làm mới gần nhất.

- **Thêm nguồn**: thêm một git repo có `.claude-plugin/marketplace.json`. Coppice tải danh mục của nó ngay.
- **Tải lại**: bấm **Refresh**, hoặc **Retry** sau khi có lỗi.
- **Xóa nguồn**: bạn xóa được mọi nguồn trừ **coppice**. Plugin đã cài từ nguồn đó vẫn được giữ. Nguồn mặc định đã xóa sẽ không quay lại.

<a id="example-github"></a>

#### Ví dụ: GitHub
1. Cài `github` từ marketplace.
2. Mở plugin, dán GitHub personal access token vào `GITHUB_PERSONAL_ACCESS_TOKEN` trong **Settings**, rồi bấm **Save settings**.
3. Bấm **Test** để xem danh sách công cụ.
4. Bật plugin và chọn agent nào được dùng.

Token này tách biệt với GitHub token trong **Settings** mà Coppice dùng cho pull request.

<a id="from-git"></a>

### Từ git
1. Mở tab **Custom**.
2. Dán **Git URL**.
3. Nếu cần, điền **Ref (optional)**, ví dụ `main`.
4. Chọn **Target directory**.
5. Bấm **Install**.

Coppice clone repo bằng thông tin đăng nhập git trên máy bạn. Sau này bấm **Update** trong chi tiết để kéo thay đổi mới về.

<a id="from-a-folder"></a>

### Từ thư mục
1. Mở tab **Custom**.
2. Trong **Plugin directories**, bấm **Browse…** và chọn thư mục, hoặc gõ đường dẫn vào **Directory path**.
3. Bấm **Add directory**.

- Thư mục này có thể là một plugin, hoặc chứa nhiều plugin trong các thư mục con.
- Sửa file xong thì bấm **Rescan**.
- Nếu hai thư mục có plugin trùng tên, plugin trong thư mục đứng trên sẽ được dùng. Dùng mũi tên để đổi thứ tự.
- Thư mục mặc định có nhãn **Default** và không xóa được.

<a id="enable-it"></a>

## 2. Bật plugin
Plugin mới thêm vào đều đang tắt.

1. Bật công tắc trên hàng plugin. Chỉ plugin ở trạng thái `ok` mới bật được.
2. Nếu chi tiết có mục **Settings**, điền từng giá trị rồi bấm **Save settings**. Các giá trị này được mã hóa khi lưu.
3. Bấm **Test** để khởi động công cụ của plugin và xem danh sách công cụ. Chưa bật plugin vẫn bấm được.

- Plugin có MCP server chạy cục bộ sẽ chạy trên máy bạn với quyền của chính máy bạn, nên Coppice sẽ hỏi bạn xác nhận trước.
- Mỗi skill có công tắc riêng. Skill nào bị tắt sẽ ẩn với mọi agent, kể cả các lần chạy đang diễn ra.

<a id="choose-which-agents-can-use-it"></a>

## 3. Chọn agent nào được dùng
Trong chi tiết, ở mục **Which agents can use it?**, chọn:

- **All agents**: mọi agent, kể cả agent tạo sau này.
- **Choose agents**: chỉ những agent bạn đánh dấu.

Plugin mới có thiết lập ban đầu như sau:

- Chỉ có skill: **All agents**.
- Có MCP server: **Choose agents**, chưa đánh dấu agent nào.

- Bạn cũng có thể chọn plugin trong form agent, ở mục **Skills**. Hai nơi luôn đồng bộ.
- Thay đổi có hiệu lực từ lần chạy tiếp theo của mỗi agent.

<a id="use-it-on-a-ticket"></a>

## 4. Dùng trên ticket
1. Bật plugin.
2. Cấp quyền cho agent được giao ticket.
3. Bấm **Run agent**.

- Agent thấy tên và mô tả của từng skill, và tải skill khi cần.
- Công cụ có tên dạng `<plugin>__<tool>`.

Để xem những gì đã được dùng:

1. Mở tab **Agent Runs** của ticket.
2. Mở rộng một lần chạy.
3. Mở **Tools & Skills**. Các lệnh gọi plugin có nhãn **Plugin**.

<a id="create-a-plugin"></a>

## 5. Tạo plugin
Plugin đơn giản nhất là một thư mục có `SKILL.md`. Muốn có tên, phiên bản và công cụ MCP thì thêm manifest.

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
| `name` | Có | Chữ, số, `.`, `_` và `-`, tối đa 64 ký tự. Bắt đầu bằng chữ hoặc số. `coppice` đã được giữ chỗ. |
| `version` | Không | Mặc định là `0.0.0`. |
| `description` | Không | Mặc định để trống. |
| `author` | Không | Một chuỗi, hoặc `{ "name": "..." }`. |
| `skills` | Không | Đường dẫn đến một thư mục skill, hoặc danh sách đường dẫn thư mục, tính từ gốc plugin. |

Không có `plugin.json`, thư mục vẫn là plugin nếu có một trong hai:

- `SKILL.md` ở cấp trên cùng.
- Skill trong `skills/`, `.agents/skills/`, `.claude/skills/` hoặc `<name>/SKILL.md`.

Plugin lấy tên thư mục và phiên bản `0.0.0`.

<a id="skills"></a>

### Skill
Mỗi skill là một thư mục có `SKILL.md`. Tên thư mục là tên skill. File bắt đầu bằng frontmatter:

```yaml
---
name: greeting-style
description: How to greet people in ticket comments. Load before writing a greeting.
---
```

- `name` và `description` là bắt buộc.
- Viết mô tả sao cho agent biết khi nào cần tải skill.

<a id="mcp-tools"></a>

### Công cụ MCP
Thêm `.mcp.json` với một map `mcpServers`. Mỗi server là cục bộ hoặc từ xa:

- Cục bộ: `command`, kèm `args` và `env` nếu cần.
- Từ xa: `"type": "http"`, `url`, kèm `headers` nếu cần.

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

- `${NAME}` trở thành một mục cài đặt trong chi tiết plugin.
- `${NAME:-default}` đặt giá trị mặc định cho mục đó.
- Coppice thay `${CLAUDE_PLUGIN_ROOT}` bằng thư mục cài plugin. Đây không phải mục cài đặt.
- Không hỗ trợ SSE server.

Ví dụ này cung cấp công cụ `hello-coppice__greet` và skill `hello-coppice:greeting-style`. Xem ví dụ đầy đủ tại [`examples/plugins`](https://github.com/fertile-org/coppice/tree/main/examples/plugins).

Các thư mục `commands/`, `agents/` và `hooks/` chưa được hỗ trợ. Chúng hiện "Not supported yet" và không chạy.

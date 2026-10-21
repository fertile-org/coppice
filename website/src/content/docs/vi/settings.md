---
title: Tham chiếu thiết lập
description: "Mọi trường bạn có thể đổi trong config.toml, kèm kiểu dữ liệu và giá trị mặc định."
---

# Tham chiếu thiết lập

Coppice lưu toàn bộ thiết lập trong một file, `config.toml`:

| Hệ điều hành | Đường dẫn |
| --- | --- |
| Linux | `~/.config/Coppice/config.toml` |
| macOS | `~/Library/Application Support/Coppice/config.toml` |

Trên Linux, nếu `XDG_CONFIG_HOME` được đặt, file nằm ở `$XDG_CONFIG_HOME/Coppice/config.toml`.

<a id="edit-it-in-the-app"></a>

## Sửa ngay trong ứng dụng
Mở **Settings** ở thanh bên. Trang này hiện nội dung file dạng văn bản, và các comment bạn viết vẫn được giữ.

- **Save** kiểm tra file trước khi ghi. Giá trị sai sẽ không được lưu.
- **Restore last good version** hoàn tác một thay đổi sai.

Có thay đổi áp dụng ngay, có thay đổi sẽ hiện "Saved. Restart Coppice to apply." (Đã lưu. Khởi động lại Coppice để áp dụng.)

Bạn cũng có thể sửa file bằng trình soạn thảo bất kỳ. Nâng cấp Coppice không làm mất những gì bạn đã sửa.

<a id="updates"></a>

## Cập nhật
Mở **Settings → Updates** để xem phiên bản và chọn **Check for updates**. **Automatic updates** bật sẵn. Tắt đi thì Coppice sẽ không tự kiểm tra phiên bản mới nữa.

Công tắc này không nằm trong `config.toml`. Coppice lưu nó dưới tên `automaticUpdates` trong `update-settings.json`, cùng thư mục.

<a id="replace-keys"></a>

## Thay khóa

Nếu đã lỡ chia sẻ bản sao lưu, hãy tạo khóa mới và mật khẩu admin mới. Token đã lưu vẫn dùng được, nhưng mọi người sẽ bị đăng xuất.

Mở **System → Backup → Replace keys**, nhập mật khẩu admin mới, và xác nhận.

<a id="logs"></a>

## Log
Admin có thể mở **System → Logs** để lọc và theo dõi trực tiếp log của server, request, các lần chạy agent và database. Mỗi lần chạy có một liên kết **View logs**.

**Export logs** lưu các dòng khớp với bộ lọc hiện tại thành `coppice-logs.zip`. Coppice đã xóa các token và khóa mà nó biết, nhưng bạn vẫn nên xem lại file trước khi gửi cho người khác.

**Report a problem** mở một GitHub issue mới, điền sẵn phiên bản Coppice của bạn. Hãy mô tả chuyện gì đã xảy ra và đáng lẽ phải ra sao, rồi đính kèm file zip từ **Export logs**.

## `[logs]`

| Trường | Kiểu | Mặc định | Tác dụng |
| --- | --- | --- | --- |
| `retention_days` | integer | `14` | Giữ log bao nhiêu ngày, tính cả hôm nay: 3, 7, 14 hoặc 30. Mặc định là 14. Coppice xóa log cũ hơn mỗi giờ, và thay đổi có hiệu lực mà không cần khởi động lại. Nếu log vượt 256 MiB, Coppice xóa log cũ nhất trước, kể cả khi chưa hết `retention_days`. |

Log mỗi ngày được chia thành các file tối đa 32 MiB, và mọi file đều tính vào giới hạn 256 MiB, kể cả file của hôm nay. Coppice không bao giờ xóa file log hay log database mà nó đang ghi. Log database được tính vào giới hạn, và Coppice xóa chúng khi quá `retention_days`.

| Trường | Kiểu | Mặc định | Tác dụng |
| --- | --- | --- | --- |
| `dir` | string | `./data/logs` | Thư mục chứa log. Ứng dụng desktop dùng thư mục riêng và bỏ qua trường này. Thay đổi cần khởi động lại. |

## `[storage]`

| Trường | Kiểu | Mặc định | Tác dụng |
| --- | --- | --- | --- |
| `run_artifacts_retention_days` | integer | `30` | Số ngày giữ console và terminal log của một lần chạy đã xong. Archive một ticket sẽ xóa ngay file của các lần chạy thuộc ticket đó. Giá trị dưới 1 được tính là 1. Thay đổi cần khởi động lại. |
| `run_console_max_bytes` | integer | `67108864` | Số byte tối đa một lần chạy được giữ cho console, và giữ thêm chừng đó cho terminal log. Phần cũ nhất bị bỏ trước. Thay đổi cần khởi động lại. |

<a id="connectors"></a>

## Connector
CLI nào Coppice tìm thấy thì được bật sẵn, trừ khi bước kiểm tra đăng nhập cho thấy bạn đã đăng xuất. Muốn tắt một CLI, chỉ cần một dòng:

```toml
[agent.connectors.claude-code]
enabled = false
```

Công tắc trong **System → Connectors** ghi đúng dòng đó.

Kilo Code, GitHub Copilot, Qwen Code và Kimi Code được bật ngay khi Coppice tìm thấy. Coppice không kiểm tra miễn phí được trạng thái đăng nhập của các CLI này, nên nếu bạn đã đăng xuất thì lần chạy sẽ báo lỗi.

## `[agent]`

| Trường | Kiểu | Mặc định | Tác dụng |
| --- | --- | --- | --- |
| `worktrees_path` | string | `<data-dir>/worktrees` | Thư mục nơi mỗi ticket có git worktree riêng. |
| `worker_count` | integer | `2` | Số job agent chạy cùng lúc. Giá trị dưới 1 được tính là 1. |
| `health_check_interval_secs` | integer | `60` | Số giây giữa các lần kiểm tra sức khỏe agent. Giá trị dưới 10 được tính là 10. |

`<data-dir>` là thư mục chứa `config.toml`: `~/.config/Coppice` trên Linux và WSL, và `~/Library/Application Support/Coppice` trên macOS.

## `[agent.connectors.<id>]`

Các id: `claude-code`, `codex`, `cursor`, `opencode`, `kilo-code`, `copilot`, `qwen-code`, `kimi`, `grok`. Bảng nào cũng có thể bỏ qua.

| Trường | Kiểu | Mặc định | Tác dụng |
| --- | --- | --- | --- |
| `enabled` | bool | không đặt: bật nếu CLI đã đăng nhập | Bật hoặc tắt connector. |
| `model_providers` | list of strings | `[]` | Các nhà cung cấp model cho connector này. Được điền khi bạn bật nó. |
| `run_timeout_secs` | integer | `1800` với OpenCode, `600` với các connector khác | Chạy quá số giây này trên connector đó thì báo lỗi. |
| `command` | string | `cursor-agent` (Cursor), `codex`, `opencode`, `kilo`, `copilot`, `qwen`, `kimi`, `grok` | File chạy của CLI. Claude Code luôn chạy `claude` và không có `command`. |

Chỉ dành cho OpenCode:

| Trường | Kiểu | Mặc định | Tác dụng |
| --- | --- | --- | --- |
| `serve_hostname` | string | `127.0.0.1` | Hostname cho tiến trình phụ của OpenCode trong lúc chạy. Cần khởi động lại. |

Đổi `command` của OpenCode cũng cần khởi động lại.

## `[workflow]`

| Trường | Kiểu | Mặc định | Tác dụng |
| --- | --- | --- | --- |
| `auto_start_runs` | bool | `true` | Khi true, giao agent cho một ticket đã có repository là tự bắt đầu chạy. Khi false, bạn bấm **Run agent**. |
| `auto_assign.default` | bool | `true` | Bật tự giao việc cho mọi trạng thái không có thiết lập riêng. |
| `auto_assign.<status>` | bool | `backlog`: `false`. Còn lại: theo `default` | Bật hoặc tắt tự giao việc cho một trạng thái. |
| `auto_split.default` | bool | `false` | Bật tự tách ticket cho mọi trạng thái không có thiết lập riêng. |
| `auto_split.<status>` | bool | theo `default` | Bật hoặc tắt tự tách ticket cho một trạng thái. |

Các khóa trạng thái: `backlog`, `ready`, `in_progress`, `in_review`, `in_qa`, `wait_for_final_review` (Wait for Human Review), `blocked`, `done`.

Khi tự giao việc bật cho trạng thái đó, Coppice giao ticket cho agent mà lần chạy đề xuất. Khi tắt, ticket hiện **Recommends:** thay vào đó.

Khi tự tách bật cho trạng thái đó, Coppice tạo các ticket con mà lần chạy đề xuất. Khi tắt, nó lưu một **Pending split recommendation** cho đến khi bạn bấm **Approve splits**.

## `[knowledge]`

| Trường | Kiểu | Mặc định | Tác dụng |
| --- | --- | --- | --- |
| `enabled` | bool | `true` | Bật hoặc tắt việc gom gọn (compaction) knowledge. |
| `poll_interval_ms` | integer | `500` | Bộ lập lịch compaction thức dậy bao lâu một lần. Giá trị dưới 100 được tính là 100. |
| `compaction.interval_secs` | integer | `1800` | Số giây giữa các vòng compaction. |
| `compaction.batch_max_tickets` | integer | `10` | Số ticket tối đa trong một lô. |
| `compaction.batch_max_source_bytes` | integer | `200000` | Số byte nguồn tối đa trong một lô. |
| `compaction.max_candidates_per_batch` | integer | `20` | Số đề xuất tối đa một lô có thể đưa ra. |
| `compaction.max_attempts` | integer | `3` | Số lần thử tối đa cho một lô. |
| `auto_save.enabled` | bool | `false` | Lưu các đề xuất ít rủi ro mà không hỏi bạn. |
| `auto_save.allowed_types` | list of strings | `[]` | Các loại được tự lưu: `bug_pattern`, `coding_convention`, `dependency_note`, `performance_note`, `review_feedback`, `test_command`. |
| `auto_save.minimum_confidence` | string | `high` | Phải giữ là `high`. |
| `retrieval.top_k` | integer | `8` | Số mục một lần tra cứu trả về. Từ 1 đến 20. |
| `retrieval.allowed_types` | list of strings | `[]` (mọi loại) | Chỉ trả về các loại này: `coding_convention`, `architecture_rule`, `bug_pattern`, `test_command`, `review_feedback`, `dependency_note`, `api_contract`, `workflow_rule`, `human_preference`, `operational_runbook`, `security_rule`, `performance_note`. |
| `retrieval.minimum_confidence` | string | `medium` | `low`, `medium` hoặc `high`. |
| `retrieval.default_page_size` | integer | `25` | Kích thước trang mặc định. |
| `retrieval.max_page_size` | integer | `100` | Kích thước trang lớn nhất. Tối đa 100. |
| `retrieval.max_active_per_board` | integer | `1000` | Số mục Knowledge nhiều nhất, đang chờ hoặc đã duyệt, mà một bảng có thể giữ. Các mục của agent trên bảng đó cũng được tính. |
| `retrieval.max_active_workspace` | integer | `10000` | Số mục Knowledge nhiều nhất, đang chờ hoặc đã duyệt, áp dụng cho cả workspace. |

Mọi giá trị số lượng và thời gian ở trên đều phải lớn hơn 0.

## `[git]`

| Trường | Kiểu | Mặc định | Tác dụng |
| --- | --- | --- | --- |
| `author_name` | string | `Coppice` | Tên tác giả trên các commit Coppice tạo. |
| `author_email` | string | `coppice@localhost` | Email tác giả trên các commit đó. |
| `push_enabled` | bool | `false` (`true` trong ứng dụng desktop) | Cho phép Coppice push branch và mở pull request. |

## `[plugins]`

| Trường | Kiểu | Mặc định | Tác dụng |
| --- | --- | --- | --- |
| `dir` | string | `<data-dir>/plugins` | Thư mục plugin mặc định. Thêm thư mục khác ở trang Plugins. |
| `allow_file_git_urls` | bool | `false` | Cho phép cài plugin từ git URL dạng `file:`. |
| `git_timeout_secs` | integer | `300` | Clone hoặc cập nhật plugin quá số giây này thì báo lỗi. |
| `mcp_start_timeout_secs` | integer | `20` | Số giây chờ MCP server của plugin khởi động. |
| `mcp_list_timeout_secs` | integer | `10` | Số giây chờ plugin liệt kê công cụ. |
| `mcp_idle_shutdown_secs` | integer | `600` | Số giây một MCP server của plugin còn chạy khi không được dùng. |

## `[mcp]`

| Trường | Kiểu | Mặc định | Tác dụng |
| --- | --- | --- | --- |
| `builtin_plugins_dir` | string | `<data-dir>/builtin-plugins` | Nơi Coppice ghi các skill có sẵn mỗi lần khởi động. Chỉnh sửa ở đó sẽ bị mất. |
| `call_timeout_secs` | integer | `60` | Một lần gọi công cụ quá số giây này thì báo lỗi. |
| `max_output_bytes` | integer | `32768` | Kết quả công cụ lớn nhất, tính bằng byte. |
| `comment_post_limit` | integer | `5` | Số bình luận tối đa một lần chạy được đăng. |
| `token_ttl_secs` | integer | `14400` | Thời hạn của token công cụ trong một lần chạy, tính bằng giây. |

## `[keybindings]`

Mỗi khóa là id của một lệnh trong [Phím tắt](/docs/shortcuts). Phím của bạn thay cho phím mặc định của lệnh đó, và để trống thì phím tắt bị tắt.

| Trường | Kiểu | Mặc định | Tác dụng |
| --- | --- | --- | --- |
| `"<command id>"` | string or list of strings | phím tắt có sẵn | Thay phím của lệnh đó. `""` xóa chúng. |

```toml
[keybindings]
"palette.open" = "P"
"ticket.create" = ""
"board.nextCard" = ["J", "Down"]
```

`Mod` là Cmd trên macOS và Ctrl trên Linux và WSL. Chuỗi phím được viết dạng `G then B`. Nhiều phím cho một lệnh được ngăn cách bằng ` or `, hoặc viết thành danh sách. Save áp dụng thay đổi mà không cần khởi động lại. Phím tắt viết sai, id lệnh không tồn tại, phím trùng với lệnh khác, hoặc phím của trình duyệt hay hệ thống sẽ bị bỏ qua. Lệnh đó giữ phím mặc định, và Settings sẽ ghi rõ lý do.

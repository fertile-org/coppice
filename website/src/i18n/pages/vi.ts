import type { Pages } from './en';

export const locale = 'vi';

/** Tiếng Việt homepage and download copy. Keys mirror `./en.ts`; anything missing stays English. */
export const pages: Pages = {
  home: {
    description:
      'Mỗi coding agent đảm nhận một vai trò, cùng lên kế hoạch, viết code và review trên một bảng. Chỉ khi bạn chấp nhận diff thì việc mới tính là xong. Chạy ngay trên máy bạn, mã nguồn mở, dùng luôn các agent CLI bạn đang trả phí.',
    h1: 'Thuê chuyên gia. Bạn là người quyết định cuối cùng.',
    lede: 'Agent PM soạn kế hoạch, bạn duyệt. Các agent chuyên môn làm song song. Chỉ khi bạn chấp nhận diff thì việc mới tính là xong.',
    hero: {
      stillAlt: 'Bảng Coppice với các ticket của repository acme-web',
      videoAlt:
        'Coppice: viết ticket, duyệt kế hoạch do agent PM soạn, để các agent viết code và test, rồi góp ý trên diff. Chỉ khi bạn chấp nhận thì việc mới tính là xong.',
      captions: [
        'Viết một ticket.',
        'Agent PM soạn kế hoạch.',
        'Bạn duyệt.',
        'Các agent viết code, review và test.',
        'Xem diff, góp ý ở bất kỳ dòng nào.',
        'Bạn chấp nhận thì mới tính là xong.',
      ],
    },
    support: 'Mã nguồn mở (Apache 2.0). Chạy ngay trên máy bạn, không cần tài khoản Coppice.',
    downloads: {
      mac: 'Tải cho macOS (Apple Silicon)',
      linux: 'Tải cho Linux (.deb)',
      docs: 'Đọc tài liệu',
    },
    howTitle: 'Cách hoạt động',
    steps: [
      {
        title: 'Giao việc.',
        body: 'Viết một ticket, hoặc duyệt ticket mà agent đề xuất. Giao cho agent PM, nó sẽ soạn kế hoạch để bạn duyệt trước khi ai đụng vào code.',
      },
      {
        title: 'Để agent bắt tay vào làm.',
        body: 'Mỗi ticket có git worktree riêng, nên các agent chạy song song không bao giờ đụng vào bản checkout chính. Bạn xem trực tiếp được mọi lần chạy.',
      },
      {
        title: 'Human Review.',
        body: 'Đọc diff ngay trong ứng dụng, góp ý ở bất kỳ dòng nào. Chỉ khi bạn chấp nhận thì việc mới tính là xong.',
      },
    ],
    howClose: 'Tải về, mở lên là dùng. Không phải cài thêm gì: không Docker, không tài khoản Coppice.',
    whyTitle: 'Vì sao chọn Coppice',
    reasons: [
      {
        title: 'Mỗi vai trò một model phù hợp.',
        body: 'Mỗi agent tự chọn CLI và model riêng. Lên kế hoạch và review bằng model mạnh nhất, viết code bằng model nhanh hơn, tất cả trên cùng một bảng.',
      },
      {
        title: 'Khỏi phải nhảy qua lại giữa các terminal.',
        body: 'Việc của mỗi agent là một thẻ trên cùng một bảng. Ticket bị kẹt và việc đang chờ bạn review hiện rõ, nên bạn biết ngay chỗ nào cần mình.',
      },
      {
        title: 'Phát hiện lỗi trước khi nó lọt vào code.',
        body: 'Mọi thay đổi đều nằm chờ bạn dưới dạng diff trong ứng dụng. Bạn chưa chấp nhận thì chưa xong, và thứ bạn chấp nhận chính là thứ được merge.',
      },
      {
        title: 'Không phải mua thêm gói nào.',
        body: 'Coppice dùng luôn các CLI bạn đang trả phí: Claude Code, Codex, Cursor, OpenCode, Kilo Code, GitHub Copilot, Qwen Code, Kimi Code và Grok.',
      },
    ],
    notTitle: 'Coppice không phải là',
    notItems: [
      'Dịch vụ viết code trên đám mây',
      'Công cụ thay thế Jira',
      'Kiểu “nói với agent một câu là có ngay sản phẩm”',
    ],
  },
  download: {
    title: 'Tải Coppice',
    intro: 'Miễn phí, chạy ngay trên máy bạn, không cần tài khoản Coppice.',
    groups: [
      {
        note: 'Không rõ máy Mac của mình loại nào? Vào menu Apple → About This Mac. Nếu thấy dòng Chip, chọn Apple Silicon. Nếu thấy dòng Processor, chọn Intel.',
        rows: [
          { name: 'macOS (Apple Silicon)', label: 'Tải .dmg' },
          { name: 'macOS (Intel)', label: 'Tải .dmg' },
        ],
      },
      {
        rows: [
          { name: 'Linux (x64)', label: 'Tải .deb' },
          { name: 'Linux (ARM64)', label: 'Tải .deb' },
        ],
      },
      {
        rows: [{ name: 'Windows 11', label: 'Cài qua WSL' }],
      },
    ],
    checksum: {
      before: 'Đối chiếu file vừa tải với ',
      sumsLabel: 'SHA256SUMS',
      between: '. Xem cách làm ở trang ',
      installLabel: 'Cài đặt',
      after: '.',
    },
    next: {
      before: 'Tiếp theo, làm theo ',
      label: 'các bước cài đặt',
      after: '.',
    },
  },
};

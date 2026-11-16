import type { Pages } from './en';

export const locale = 'vi';

/** Tiếng Việt homepage and download copy. Keys mirror `./en.ts`; anything missing stays English. */
export const pages: Pages = {
  home: {
    title: 'Coppice: ứng dụng desktop cho multi-agent coding',
    description:
      'Coppice là ứng dụng desktop cho multi-agent coding. Bạn viết ticket, duyệt kế hoạch và chấp nhận diff. Claude Code, Codex và các CLI khác chạy trên máy bạn.',
    worksWith: 'Dùng được với',
    h1: 'Thuê chuyên gia. Bạn là người quyết định cuối cùng.',
    lede: 'Ứng dụng desktop cho multi-agent coding. Agent PM soạn kế hoạch, bạn duyệt. Các agent chuyên môn làm song song. Chỉ khi bạn chấp nhận diff thì việc mới tính là xong.',
    hero: {
      stillAlt: 'Bảng Coppice với các ticket của repository acme-web',
      videoAlt:
        'Coppice: viết ticket, so sánh các phương án thiết kế, duyệt kế hoạch do agent PM soạn, để các agent viết code và test, rồi góp ý trên diff. Chỉ khi bạn chấp nhận thì việc mới tính là xong.',
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
        title: 'Lên kế hoạch với agent PM',
        body: 'Agent PM đọc ticket và soạn kế hoạch: cách làm, các bước, và cách kiểm tra thay đổi. Bạn duyệt kế hoạch trước khi có code được viết.',
        imageAlt:
          'Plan Review cho nút chuyển dark mode. Agent PM là người được giao, và kế hoạch đề xuất ghi cách làm cùng các bước.',
      },
      {
        title: 'Duyệt thiết kế',
        body: 'Khi ticket đổi phần người dùng nhìn thấy, agent đưa ra vài bố cục và đánh dấu phương án nó đề xuất. Bạn chọn một, và kế hoạch đi theo lựa chọn đó.',
        imageAlt: 'Các phương án thiết kế cho nút theme trong Settings, với ba bố cục và bản xem trước của phương án được đề xuất.',
      },
      {
        title: 'Để các agent làm việc',
        body: 'Các agent chuyên môn viết code, review và test ticket. Mỗi ticket có git worktree riêng, nên việc làm song song không đụng bản checkout chính. Bạn xem được một lần chạy khi nó đang diễn ra.',
        imageAlt: 'Bảng, với ticket đang làm, đang review, đang QA, và ticket đang chờ một người.',
      },
      {
        title: 'Xem các thay đổi trong code',
        body: 'Đọc diff ngay trong ứng dụng và góp ý ở bất kỳ dòng nào. Chỉ khi bạn chấp nhận thì việc mới tính là xong. Accept và Merge là hai lựa chọn riêng.',
        imageAlt: 'Review code của nút theme: kế hoạch đã duyệt, file đã đổi, và một góp ý trên một dòng của diff.',
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
        title: 'Không phải chuyển qua lại giữa các terminal.',
        body: 'Việc của mỗi agent là một thẻ trên cùng một bảng. Ticket bị kẹt và việc đang chờ bạn review hiện rõ, nên bạn biết ngay chỗ nào cần bạn.',
      },
      {
        title: 'Phát hiện lỗi trước khi lỗi đó lọt vào code.',
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
    metaTitle: 'Tải Coppice cho macOS và Linux',
    description:
      'Tải Coppice, ứng dụng desktop miễn phí, mã nguồn mở cho multi-agent coding, cho macOS và Linux. Chạy trên máy bạn, không cần tài khoản.',
    intro: 'Miễn phí, chạy ngay trên máy bạn, không cần tài khoản Coppice.',
    groups: [
      {
        note: 'Không rõ máy Mac của bạn loại nào? Vào menu Apple → About This Mac. Nếu thấy dòng Chip, chọn Apple Silicon. Nếu thấy dòng Processor, chọn Intel.',
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

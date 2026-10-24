import type { Ui } from '../ui';

export const locale = 'vi';

/** Tiếng Việt UI chrome. Keys mirror `./en.ts`; this replaces the whole object. */
export const ui: Ui = {
  languageLabel: 'Ngôn ngữ',
  skip: 'Chuyển đến nội dung',
  nav: {
    docs: 'Tài liệu',
    github: 'GitHub',
    download: 'Tải xuống',
  },
  footer: {
    tagline: 'Thuê chuyên gia. Bạn là người quyết định cuối cùng.',
    licenseBefore: 'Coppice là phần mềm mã nguồn mở theo ',
    licenseLabel: 'giấy phép Apache 2.0',
    licenseAfter: '.',
    docs: 'Tài liệu',
    install: 'Cài đặt',
    concepts: 'Khái niệm',
    providers: 'Nhà cung cấp',
    faq: 'Hỏi đáp',
    github: 'GitHub',
  },
  docs: {
    navLabel: 'Tài liệu',
    breadcrumbLabel: 'Đường dẫn',
    pagerLabel: 'Tài liệu khác',
    previous: 'Trang trước',
    next: 'Trang sau',
    install: 'Cài đặt',
    gettingStarted: 'Bắt đầu',
    concepts: 'Khái niệm',
    providers: 'Nhà cung cấp',
    plugins: 'Plugin',
    faq: 'Hỏi đáp',
    advanced: 'Nâng cao',
    settings: 'Thiết lập',
    shortcuts: 'Phím tắt',
    backup: 'Sao lưu dữ liệu',
  },
  search: {
    placeholder: 'Tìm trong tài liệu',
    label: 'Tìm trong tài liệu',
    hint: '/',
    footer: '↑↓ để di chuyển, Enter để mở, Esc để đóng',
    close: 'Đóng',
    noResults: 'Không có kết quả cho “{query}”.',
  },
};

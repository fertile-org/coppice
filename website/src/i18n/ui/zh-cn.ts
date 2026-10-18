import type { Ui } from '../ui';

export const locale = 'zh-cn';

/** 简体中文 UI chrome. Keys mirror `./en.ts`; this replaces the whole object. */
export const ui: Ui = {
  languageLabel: '语言',
  skip: '跳到正文',
  nav: {
    docs: '文档',
    github: 'GitHub',
    download: '下载',
  },
  footer: {
    tagline: '雇用专家，你来拍板。',
    licenseBefore: 'Coppice 基于 ',
    licenseLabel: 'Apache 2.0 许可证',
    licenseAfter: '开源。',
    docs: '文档',
    install: '安装',
    concepts: '核心概念',
    providers: '提供商',
    faq: '常见问题',
    github: 'GitHub',
  },
  docs: {
    navLabel: '文档',
    install: '安装',
    gettingStarted: '快速上手',
    concepts: '核心概念',
    providers: '提供商',
    plugins: '插件',
    faq: '常见问题',
    advanced: '进阶',
    settings: '设置',
    shortcuts: '键盘快捷键',
    backup: '备份数据',
  },
  search: {
    placeholder: '搜索文档',
    label: '搜索文档',
    hint: '/',
    footer: '↑↓ 移动，Enter 打开，Esc 关闭',
    close: '关闭',
    noResults: '没有找到与“{query}”相关的结果。',
  },
};

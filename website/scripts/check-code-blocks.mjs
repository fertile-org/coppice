import path from 'node:path';
import { chromium } from 'playwright';
import { startStaticServer } from './static-server.mjs';

const dist = path.resolve('dist');
const pages = ['/', '/docs', '/docs/install', '/docs/concepts', '/docs/providers', '/docs/faq'];
const viewports = [
  { width: 1440, height: 900 },
  { width: 390, height: 844 },
];

const fences = {
  powershell: 'wsl --update\nwsl --shutdown',
  sh: [
    'cd ~',
    'wget https://github.com/fertile-org/coppice/releases/download/v<version>/Coppice-<version>-linux-amd64.deb',
    'sudo apt install ./Coppice-<version>-linux-amd64.deb',
  ].join('\n'),
};

const server = await startStaticServer(dist, { style: 'clean' });
const browser = await chromium.launch();
const failures = [];

function fail(message) {
  failures.push(message);
}

function parseColor(value) {
  const match = String(value).match(/rgba?\(\s*([\d.]+)[,\s]+([\d.]+)[,\s]+([\d.]+)/);
  if (!match) return null;
  return [Number(match[1]), Number(match[2]), Number(match[3])];
}

function channel(c) {
  const s = c / 255;
  return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
}

function contrast(a, b) {
  const lum = (rgb) => 0.2126 * channel(rgb[0]) + 0.7152 * channel(rgb[1]) + 0.0722 * channel(rgb[2]);
  const l1 = lum(a);
  const l2 = lum(b);
  const [hi, lo] = l1 > l2 ? [l1, l2] : [l2, l1];
  return (hi + 0.05) / (lo + 0.05);
}

function featuresOff(value) {
  const text = String(value).toLowerCase();
  return /liga["'\s]+0/.test(text) && /calt["'\s]+0/.test(text);
}

try {
  for (const viewport of viewports) {
    const context = await browser.newContext({ viewport });
    for (const urlPath of pages) {
      const page = await context.newPage();
      await page.goto(new URL(urlPath, server.url).href, { waitUntil: 'networkidle' });
      const report = await page.evaluate(() => {
        function painted(el) {
          let node = el;
          while (node) {
            const bg = getComputedStyle(node).backgroundColor;
            const match = bg.match(/rgba?\(\s*([\d.]+)[,\s]+([\d.]+)[,\s]+([\d.]+)(?:[,\s/]+([\d.]+))?/);
            if (match) {
              const alpha = match[4] === undefined ? 1 : Number(match[4]);
              if (alpha > 0.05) return bg;
            }
            node = node.parentElement;
          }
          return null;
        }

        function chain(el) {
          const clipped = [];
          let node = el.parentElement;
          while (node && node !== document.documentElement) {
            const overflowX = getComputedStyle(node).overflowX;
            if (overflowX === 'hidden' || overflowX === 'clip') {
              clipped.push(`${node.tagName}.${String(node.className || '')}`.trim());
            }
            node = node.parentElement;
          }
          return clipped;
        }

        const blocks = [...document.querySelectorAll('pre')].map((pre) => {
          const code = pre.querySelector('code');
          const codeStyle = code ? getComputedStyle(code) : null;
          const preStyle = getComputedStyle(pre);
          const spans = [...pre.querySelectorAll('span')].map((span) => {
            const style = getComputedStyle(span);
            return {
              text: span.textContent,
              color: style.color,
              background: style.backgroundColor,
              padding: style.padding,
              borderTopWidth: style.borderTopWidth,
              borderRadius: style.borderRadius,
              painted: painted(span),
              ligatures: style.fontVariantLigatures,
              features: style.fontFeatureSettings,
            };
          });
          return {
            lang: pre.dataset.language || '',
            className: pre.className,
            text: pre.innerText.replace(/\r\n/g, '\n'),
            pre: {
              overflowX: preStyle.overflowX,
              color: preStyle.color,
              background: preStyle.backgroundColor,
              ligatures: preStyle.fontVariantLigatures,
              features: preStyle.fontFeatureSettings,
              clientWidth: pre.clientWidth,
              scrollWidth: pre.scrollWidth,
            },
            code: codeStyle && {
              background: codeStyle.backgroundColor,
              color: codeStyle.color,
              padding: codeStyle.padding,
              borderTopWidth: codeStyle.borderTopWidth,
              borderRadius: codeStyle.borderRadius,
              ligatures: codeStyle.fontVariantLigatures,
              features: codeStyle.fontFeatureSettings,
            },
            spans,
            clippedBy: chain(pre),
          };
        });

        const inline = [...document.querySelectorAll('code')]
          .filter((code) => !code.closest('pre'))
          .map((code) => {
            const style = getComputedStyle(code);
            const parentSize = parseFloat(getComputedStyle(code.parentElement).fontSize);
            return {
              text: code.textContent,
              background: style.backgroundColor,
              padding: style.padding,
              borderRadius: style.borderRadius,
              fontFamily: style.fontFamily,
              fontSize: parseFloat(style.fontSize),
              parentSize,
              ligatures: style.fontVariantLigatures,
              features: style.fontFeatureSettings,
            };
          });

        return { blocks, inline };
      });

      const label = `${urlPath} @ ${viewport.width}px`;
      if (urlPath === '/docs/install') {
        const langs = report.blocks.map((block) => block.lang).sort();
        if (langs.join(',') !== 'powershell,sh') {
          fail(`${label}: expected powershell and sh fences, found ${langs.join(',') || 'none'}`);
        }
      }

      for (const block of report.blocks) {
        const where = `${label} pre.${block.lang || 'code'}`;
        if (!String(block.className).includes('astro-code')) {
          fail(`${where}: missing astro-code class (${block.className})`);
        }
        if (block.pre.overflowX !== 'auto' && block.pre.overflowX !== 'scroll') {
          fail(`${where}: pre overflow-x is ${block.pre.overflowX}`);
        }
        if (block.pre.ligatures !== 'none') {
          fail(`${where}: pre ligatures are ${block.pre.ligatures}`);
        }
        if (!featuresOff(block.pre.features)) {
          fail(`${where}: pre font-feature-settings are ${block.pre.features}`);
        }
        if (block.clippedBy.length) {
          fail(`${where}: clipped by ${block.clippedBy.join(' > ')}`);
        }
        if (!block.code) {
          fail(`${where}: missing code element`);
          continue;
        }
        if (block.code.background !== 'rgba(0, 0, 0, 0)') {
          fail(`${where}: code background is ${block.code.background}`);
        }
        if (block.code.padding !== '0px') {
          fail(`${where}: code padding is ${block.code.padding}`);
        }
        if (block.code.borderTopWidth !== '0px' || block.code.borderRadius !== '0px') {
          fail(`${where}: code border is ${block.code.borderTopWidth} / radius ${block.code.borderRadius}`);
        }
        if (block.code.color !== block.pre.color) {
          fail(`${where}: code color ${block.code.color} does not inherit pre color ${block.pre.color}`);
        }
        if (block.code.ligatures !== 'none') {
          fail(`${where}: code ligatures are ${block.code.ligatures}`);
        }
        if (!featuresOff(block.code.features)) {
          fail(`${where}: code font-feature-settings are ${block.code.features}`);
        }
        const expected = fences[block.lang];
        if (expected && block.text !== expected) {
          fail(`${where}: copied text ${JSON.stringify(block.text)} !== ${JSON.stringify(expected)}`);
        }
        for (const span of block.spans) {
          if (span.background !== 'rgba(0, 0, 0, 0)') {
            fail(`${where}: span background is ${span.background} (${JSON.stringify(span.text)})`);
          }
          if (span.padding !== '0px' || span.borderTopWidth !== '0px' || span.borderRadius !== '0px') {
            fail(`${where}: span has inline-code chrome (${span.padding}, ${span.borderRadius})`);
          }
          if (span.ligatures !== 'none') {
            fail(`${where}: span ligatures are ${span.ligatures}`);
          }
          const fg = parseColor(span.color);
          const bg = parseColor(span.painted);
          if (!fg || !bg) {
            fail(`${where}: could not read span colors ${span.color} on ${span.painted}`);
            continue;
          }
          const ratio = contrast(fg, bg);
          if (ratio < 4.5) {
            fail(`${where}: contrast ${ratio.toFixed(2)} for ${span.color} on ${span.painted} (${JSON.stringify(span.text)})`);
          }
        }
      }

      for (const code of report.inline) {
        const where = `${label} inline ${JSON.stringify(code.text)}`;
        if (code.background !== 'rgb(245, 240, 230)') {
          fail(`${where}: background is ${code.background}`);
        }
        const padding = code.padding.split(' ').map((part) => parseFloat(part));
        const vertical = padding[0];
        const horizontal = padding.length > 1 ? padding[1] : padding[0];
        if (!(vertical > 0 && horizontal > 0)) {
          fail(`${where}: padding is ${code.padding}`);
        }
        if (code.borderRadius !== '4px') {
          fail(`${where}: radius is ${code.borderRadius}`);
        }
        if (!code.fontFamily.includes('ui-monospace')) {
          fail(`${where}: font is ${code.fontFamily}`);
        }
        if (Math.abs(code.fontSize / code.parentSize - 0.92) > 0.03) {
          fail(`${where}: font-size ratio is ${code.fontSize / code.parentSize}`);
        }
        if (code.ligatures !== 'none') {
          fail(`${where}: ligatures are ${code.ligatures}`);
        }
        if (!featuresOff(code.features)) {
          fail(`${where}: font-feature-settings are ${code.features}`);
        }
      }

      if (urlPath === '/docs/install') {
        const scrolled = await page.locator('pre[data-language="sh"]').evaluate((el) => {
          el.scrollLeft = el.scrollWidth;
          const line = [...el.querySelectorAll('span.line')].find((node) => node.textContent.includes('wget'));
          const lineRect = line.getBoundingClientRect();
          const preRect = el.getBoundingClientRect();
          const style = getComputedStyle(el);
          const visibleRight = preRect.right - parseFloat(style.borderRightWidth) - parseFloat(style.paddingRight);
          const visibleLeft = preRect.left + parseFloat(style.borderLeftWidth) + parseFloat(style.paddingLeft);
          return {
            clientWidth: el.clientWidth,
            scrollWidth: el.scrollWidth,
            scrollLeft: el.scrollLeft,
            endVisible: lineRect.right <= visibleRight + 2 && lineRect.right >= visibleLeft,
            onScreen: preRect.left >= -1 && preRect.right <= window.innerWidth + 1 && lineRect.right <= window.innerWidth + 2,
            lineRight: lineRect.right,
            preRight: preRect.right,
            innerWidth: window.innerWidth,
            pageOverflow: document.documentElement.scrollWidth - document.documentElement.clientWidth,
            visibleRight,
            text: el.innerText.replace(/\r\n/g, '\n'),
          };
        });
        if (!(scrolled.scrollWidth > scrolled.clientWidth + 8)) {
          fail(`${label}: wget block does not scroll (${scrolled.scrollWidth} <= ${scrolled.clientWidth})`);
        }
        if (scrolled.scrollLeft <= 0 || !scrolled.endVisible || !scrolled.onScreen || scrolled.pageOverflow > 1) {
          fail(`${label}: wget end is not reachable by scrolling (${JSON.stringify(scrolled)})`);
        }
        if (scrolled.text !== fences.sh) {
          fail(`${label}: scrolled block text changed ${JSON.stringify(scrolled.text)}`);
        }
      }

      await page.close();
    }
    await context.close();
  }
} finally {
  await browser.close();
  await server.close();
}

if (failures.length) {
  console.error(`Code block check failed (${failures.length}):`);
  for (const message of failures) console.error(`- ${message}`);
  process.exit(1);
}

console.log(`Code block styles passed on ${pages.length} pages at ${viewports.map((v) => v.width).join(' and ')}px.`);

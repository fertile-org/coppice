/**
 * The three design options the PM agent "publishes" in the hero take. Each is
 * a self-contained page, like the ones agents write with design__write. Keep
 * dark surfaces in the top half: capture-hero.mjs looks for its dark caption
 * pill in the bottom of the frame.
 */

const STYLE = `
  * { box-sizing: border-box; margin: 0; }
  body { background: #f6f1e9; color: #292524; font: 15px/1.45 Roboto, "Helvetica Neue", Arial, sans-serif; }
  main { max-width: 860px; margin: 0 auto; padding: 36px 32px; }
  h1 { font-size: 28px; font-weight: 600; letter-spacing: -0.02em; }
  .lede { margin-top: 4px; color: #78716c; }
  .panel { margin-top: 24px; background: #fffdf9; border: 1px solid #e3dbcf; border-radius: 12px; }
  .row { display: flex; align-items: center; justify-content: space-between; gap: 24px; padding: 18px 22px; }
  .row + .row { border-top: 1px solid #eee7dc; }
  .name { font-weight: 600; }
  .hint { color: #78716c; font-size: 14px; }
  .value { color: #57534e; }
`;

function shell(title, body, head) {
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>${title}</title>
${head}
</head>
<body>
<main>
<h1>Settings</h1>
<p class="lede">Preferences for acme-web on this machine.</p>
${body}
</main>
</body>
</html>
`;
}

function page(title, body, extraStyle = '') {
  return shell(title, body, `<style>${STYLE}${extraStyle}</style>`);
}

const OTHER_ROWS = `
  <div class="row">
    <div><div class="name">Language</div><div class="hint">Used for dates and labels.</div></div>
    <div class="value">English</div>
  </div>
  <div class="row">
    <div><div class="name">Start page</div><div class="hint">What opens first.</div></div>
    <div class="value">Dashboard</div>
  </div>`;

const SWITCH_BODY = `<section class="panel">
  <div class="row">
    <div><div class="name">Dark mode</div><div class="hint">Use the dark theme on this machine.</div></div>
    <div class="switch" role="img" aria-label="Dark mode on"></div>
  </div>${OTHER_ROWS}
</section>`;

const SWITCH_CSS = `${STYLE}
.switch { position: relative; width: 52px; height: 30px; border-radius: 15px; background: #2f6b4f; }
   .switch::after { content: ""; position: absolute; top: 3px; right: 3px; width: 24px; height: 24px; border-radius: 50%; background: #fff; }`;

const SWITCH = shell(
  'Switch in the Settings row',
  SWITCH_BODY,
  `<link rel="stylesheet" href="style.css">`,
);

const SEGMENTED = page(
  'Light, Dark and System buttons',
  `<section class="panel">
  <div class="row">
    <div><div class="name">Theme</div><div class="hint">System follows your computer's setting.</div></div>
    <div class="segments">
      <span>Light</span><span class="on">Dark</span><span>System</span>
    </div>
  </div>${OTHER_ROWS}
</section>`,
  `.segments { display: flex; padding: 4px; gap: 4px; border-radius: 10px; background: #eee7dc; }
   .segments span { padding: 7px 20px; border-radius: 7px; color: #57534e; font-weight: 500; }
   .segments .on { background: #2f6b4f; color: #fff; }`,
);

const CARDS = page(
  'Theme preview cards',
  `<section class="panel">
  <div class="row"><div><div class="name">Theme</div><div class="hint">Pick the look you want to work in.</div></div></div>
  <div class="cards">
    <div class="card">
      <div class="shot light"><i></i><b></b><b></b><b></b></div>
      <div class="label">Light</div>
    </div>
    <div class="card on">
      <div class="shot dark"><i></i><b></b><b></b><b></b></div>
      <div class="label">Dark</div>
    </div>
  </div>
  <div class="row">
    <div><div class="name">Language</div><div class="hint">Used for dates and labels.</div></div>
    <div class="value">English</div>
  </div>
</section>`,
  `.cards { display: grid; grid-template-columns: 1fr 1fr; gap: 18px; padding: 0 22px 22px; }
   .card { padding: 10px; border: 1px solid #e3dbcf; border-radius: 12px; }
   .card.on { border: 2px solid #2f6b4f; padding: 9px; }
   .shot { display: grid; gap: 9px; height: 150px; padding: 16px; border-radius: 8px; align-content: start; }
   .shot i { width: 40%; height: 14px; border-radius: 4px; }
   .shot b { height: 10px; border-radius: 4px; }
   .light { background: #f6f1e9; } .light i { background: #a8a29e; } .light b { background: #ddd5c8; }
   .dark { background: #292524; } .dark i { background: #a8a29e; } .dark b { background: #57534e; }
   .label { margin-top: 8px; text-align: center; font-weight: 600; }`,
);

export const DESIGN_SET_TITLE = 'Dark mode toggle on Settings';
export const RECOMMENDED_OPTION_ID = 'option-a';

export const DESIGN_OPTIONS = [
  {
    id: 'option-a',
    title: 'Switch in the Settings row',
    entry: 'option-a/index.html',
    html: SWITCH,
    files: [{ path: 'option-a/style.css', content: SWITCH_CSS }],
  },
  { id: 'option-b', title: 'Light, Dark and System buttons', entry: 'option-b/index.html', html: SEGMENTED },
  { id: 'option-c', title: 'Theme preview cards', entry: 'option-c/index.html', html: CARDS },
];

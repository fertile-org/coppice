import fs from 'node:fs';
import path from 'node:path';

const dist = path.resolve('dist');
const downloadPath = path.join(dist, 'download.html');
const installPath = path.join(dist, 'docs', 'install.html');
const indexPath = path.join(dist, 'index.html');

const failures = [];

function read(file) {
  if (!fs.existsSync(file)) {
    failures.push(`missing ${path.relative(process.cwd(), file)}`);
    return '';
  }
  return fs.readFileSync(file, 'utf8');
}

function expectOrder(html, needles, label) {
  let from = 0;
  for (const needle of needles) {
    const at = html.indexOf(needle, from);
    if (at < 0) {
      failures.push(`${label} missing ${JSON.stringify(needle)}`);
      return;
    }
    from = at + needle.length;
  }
}

const macHelp =
  'Not sure which Mac you have? Open Apple menu → About This Mac. If it lists a Chip, choose Apple Silicon. If it lists a Processor, choose Intel.';

const download = read(downloadPath);
expectOrder(
  download,
  [
    'Download Coppice',
    'Free, runs on your own machine, with no Coppice account.',
    'data-os="mac"',
    'macOS (Apple Silicon)',
    'https://github.com/fertile-org/coppice/releases/latest/download/Coppice-mac-arm64.dmg',
    'Download .dmg',
    'macOS (Intel)',
    'https://github.com/fertile-org/coppice/releases/latest/download/Coppice-mac-x64.dmg',
    macHelp,
    'data-os="linux"',
    'Linux (x64)',
    'https://github.com/fertile-org/coppice/releases/latest/download/Coppice-linux-amd64.deb',
    'Download .deb',
    'Linux (ARM64)',
    'https://github.com/fertile-org/coppice/releases/latest/download/Coppice-linux-arm64.deb',
    'data-os="windows"',
    'Windows 11',
    '/docs/install#windows-11-through-wsl',
    'Install through WSL',
    'Check your download against ',
    'https://github.com/fertile-org/coppice/releases/latest/download/SHA256SUMS',
    'SHA256SUMS',
    '. See ',
    '/docs/install',
    'Install',
    ' for how.',
    'Next, follow the ',
    '/docs/install',
    'install steps',
  ],
  'download.html',
);

if (download && /mock|fake/i.test(download.replace(/<script[\s\S]*?<\/script>/gi, ''))) {
  failures.push('download.html mentions mock or fake outside scripts');
}

const install = read(installPath);
if (install && !install.includes('id="windows-11-through-wsl"')) {
  failures.push('install.html is missing #windows-11-through-wsl');
}
for (const file of [
  'Coppice-mac-arm64.dmg',
  'Coppice-mac-x64.dmg',
  'Coppice-linux-amd64.deb',
  'Coppice-linux-arm64.deb',
]) {
  const href = `https://github.com/fertile-org/coppice/releases/latest/download/${file}`;
  if (install && !install.includes(href)) failures.push(`install.html missing ${href}`);
}
if (install && !install.includes('You can also get every build from')) {
  failures.push('install.html is missing the GitHub Releases sentence');
}
if (install.includes('including the Intel Mac and Linux arm64 builds')) {
  failures.push('install.html still has the old GitHub Releases sentence');
}

const index = read(indexPath);
if (index && !index.includes('href="/download"')) failures.push('index.html Download buttons do not link to /download');
if (index.includes('href="#"')) failures.push('index.html still has href="#"');
if (index.includes('0.1.0-rc.7')) failures.push('index.html still pins v0.1.0-rc.7');
// The old Linux asset name, built at runtime so the repo never spells it.
const oldLinuxDeb = `${['linux', 'x64'].join('-')}.deb`;
if (index.includes(oldLinuxDeb)) failures.push(`index.html still mentions ${oldLinuxDeb}`);

if (failures.length) {
  console.error('Download page check failed:');
  for (const failure of failures) console.error(`  ${failure}`);
  process.exit(1);
}

console.log('Download page check passed.');

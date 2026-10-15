import fs from 'node:fs';
import path from 'node:path';

const src = path.resolve('src');
const needles = ['[screenshot:', '<!-- QUESTION'];

function files(dir, out = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) files(full, out);
    else if (entry.isFile()) out.push(full);
  }
  return out;
}

const hits = [];
for (const file of files(src)) {
  const lines = fs.readFileSync(file, 'utf8').split(/\r?\n/);
  lines.forEach((line, index) => {
    if (needles.some((needle) => line.includes(needle))) {
      hits.push(`${path.relative(process.cwd(), file)}:${index + 1}`);
    }
  });
}

if (hits.length) {
  for (const hit of hits) console.error(hit);
  process.exit(1);
}

console.log('No doc placeholders in website/src.');

import fs from 'node:fs';
import path from 'node:path';

const root = path.resolve(process.argv[2] || 'dist');

function htmlFiles(dir, out = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) htmlFiles(full, out);
    else if (entry.name.endsWith('.html')) out.push(full);
  }
  return out;
}

const files = htmlFiles(root).sort((a, b) => b.length - a.length);
for (const file of files) {
  const name = path.basename(file);
  if (name === 'index.html' || name === '404.html') continue;
  const dir = path.join(path.dirname(file), name.slice(0, -'.html'.length));
  fs.mkdirSync(dir, { recursive: true });
  const dest = path.join(dir, 'index.html');
  fs.renameSync(file, dest);
  console.log(`${path.relative(root, file)} -> ${path.relative(root, dest)}`);
}

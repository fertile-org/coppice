import fs from 'node:fs';
import path from 'node:path';

export function createRotatingLog(file, { maxBytes = 10 * 1024 * 1024, keep = 3 } = {}) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  let fd = fs.openSync(file, 'a');
  let size = fs.fstatSync(fd).size;

  function rotate() {
    fs.closeSync(fd);
    fs.rmSync(`${file}.${keep}`, { force: true });
    for (let i = keep - 1; i >= 1; i -= 1) {
      if (fs.existsSync(`${file}.${i}`)) {
        fs.renameSync(`${file}.${i}`, `${file}.${i + 1}`);
      }
    }
    if (keep > 0) {
      fs.renameSync(file, `${file}.1`);
    } else {
      fs.rmSync(file, { force: true });
    }
    fd = fs.openSync(file, 'a');
    size = 0;
  }

  return {
    write(chunk) {
      if (fd === null) return;
      const buf = Buffer.isBuffer(chunk) ? chunk : Buffer.from(String(chunk));
      fs.writeSync(fd, buf);
      size += buf.length;
      if (size >= maxBytes) rotate();
    },
    close() {
      if (fd === null) return;
      fs.closeSync(fd);
      fd = null;
    },
  };
}

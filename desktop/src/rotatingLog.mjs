import fs from 'node:fs';
import path from 'node:path';

/** Logging must never take the app down: the first I/O failure is reported once and file logging stops. */
export function createRotatingLog(file, { maxBytes = 10 * 1024 * 1024, keep = 3 } = {}) {
  let fd = null;
  let size = 0;

  function fail(err) {
    console.error(`coppice: disabling ${file} logging:`, err);
    if (fd !== null) {
      try {
        fs.closeSync(fd);
      } catch {
        /* already broken */
      }
    }
    fd = null;
  }

  try {
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fd = fs.openSync(file, 'a');
    size = fs.fstatSync(fd).size;
  } catch (err) {
    fail(err);
  }

  function rotate() {
    fs.closeSync(fd);
    fd = null;
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
      try {
        const buf = Buffer.isBuffer(chunk) ? chunk : Buffer.from(String(chunk));
        fs.writeSync(fd, buf);
        size += buf.length;
        if (size >= maxBytes) rotate();
      } catch (err) {
        fail(err);
      }
    },
    close() {
      if (fd === null) return;
      try {
        fs.closeSync(fd);
      } catch {
        /* nothing left to do */
      }
      fd = null;
    },
  };
}

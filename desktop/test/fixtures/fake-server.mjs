// Stand-in for `coppice-server desktop`, driven by argv[2]:
//   ready (default)  — logs, prints the ready line (split across writes), exits 0 on SIGTERM
//   exit-early       — exits 3 before ready
//   never-ready      — logs but never prints the ready line
//   ignore-term      — prints the ready line, ignores SIGTERM and stdin EOF
//   grandchild       — like ready, but first spawns a `sleep` that inherits stdout/stderr
//   grandchild-exit  — spawns that `sleep`, then exits 5 before ready
import { spawn } from 'node:child_process';

const mode = process.argv[2] ?? 'ready';

process.stdout.write('INFO starting fake server\n');
process.stderr.write('WARN fake stderr line\n');

if (mode === 'grandchild' || mode === 'grandchild-exit') {
  const sleeper = spawn('sleep', ['30'], { stdio: ['ignore', 'inherit', 'inherit'] });
  process.stdout.write(`GRANDCHILD pid=${sleeper.pid}\n`);
  sleeper.unref();
}

if (mode === 'exit-early') {
  process.stderr.write('Error: fake failure\n');
  process.exit(3);
}

if (mode === 'grandchild-exit') {
  process.exit(5);
}

if (mode === 'ignore-term') {
  process.on('SIGTERM', () => {});
} else {
  process.on('SIGTERM', () => process.exit(0));
}

if (mode === 'ready' || mode === 'ignore-term' || mode === 'grandchild') {
  process.stdout.write('COPPICE_READY url=http://127.0');
  setTimeout(() => process.stdout.write('.0.1:5123\n'), 20);
}

setInterval(() => {}, 1000);

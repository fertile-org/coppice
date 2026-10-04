// Stand-in for `coppice-server desktop`, driven by argv[2]:
//   ready (default) — logs, prints the ready line (split across writes), exits 0 on SIGTERM
//   exit-early      — exits 3 before ready
//   never-ready     — logs but never prints the ready line
//   ignore-term     — prints the ready line, ignores SIGTERM and stdin EOF
const mode = process.argv[2] ?? 'ready';

process.stdout.write('INFO starting fake server\n');
process.stderr.write('WARN fake stderr line\n');

if (mode === 'exit-early') {
  process.stderr.write('Error: fake failure\n');
  process.exit(3);
}

if (mode === 'ignore-term') {
  process.on('SIGTERM', () => {});
} else {
  process.on('SIGTERM', () => process.exit(0));
}

if (mode === 'ready' || mode === 'ignore-term') {
  process.stdout.write('COPPICE_READY url=http://127.0');
  setTimeout(() => process.stdout.write('.0.1:5123\n'), 20);
}

setInterval(() => {}, 1000);

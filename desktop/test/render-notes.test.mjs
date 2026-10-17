import assert from 'node:assert/strict';
import { describe, it } from 'node:test';

import { renderNotes, spliceInstallNotes } from '../scripts/render-notes.mjs';

const template = [
  '## Install',
  '',
  '{{MAC_UNSIGNED}}',
  'Unsigned: run xattr.',
  '{{/MAC_UNSIGNED}}',
  '',
  'Linux: apt install.',
  '',
].join('\n');

describe('renderNotes', () => {
  it('keeps the block and strips the marker lines when the mac build is unsigned', () => {
    assert.equal(
      renderNotes(template, { macUnsigned: true }),
      '## Install\n\nUnsigned: run xattr.\n\nLinux: apt install.\n',
    );
  });

  it('removes the block entirely when the mac build is signed', () => {
    assert.equal(renderNotes(template, { macUnsigned: false }), '## Install\n\nLinux: apt install.\n');
  });

  it('handles several blocks', () => {
    const t = '{{MAC_UNSIGNED}}\na\n{{/MAC_UNSIGNED}}\nb\n{{MAC_UNSIGNED}}\nc\n{{/MAC_UNSIGNED}}\n';
    assert.equal(renderNotes(t, { macUnsigned: true }), 'a\nb\nc\n');
    assert.equal(renderNotes(t, { macUnsigned: false }), 'b\n');
  });

  it('splices the install section onto generated notes and replaces it on the next run', () => {
    const changelog = '## Changelog\n\n- fixed a bug\n';
    const unsigned = spliceInstallNotes(changelog, renderNotes(template, { macUnsigned: true }));
    assert.match(unsigned, /^## Changelog\n\n- fixed a bug\n\n## Install\n/);
    assert.match(unsigned, /Unsigned: run xattr\./);
    assert.doesNotMatch(unsigned, /\{\{MAC_UNSIGNED\}\}/);

    const signed = spliceInstallNotes(unsigned, renderNotes(template, { macUnsigned: false }));
    assert.match(signed, /^## Changelog\n\n- fixed a bug\n\n## Install\n/);
    assert.doesNotMatch(signed, /Unsigned/);
    assert.equal(spliceInstallNotes(signed, renderNotes(template, { macUnsigned: false })), signed);
    assert.equal(spliceInstallNotes('', renderNotes(template, { macUnsigned: false })), '## Install\n\nLinux: apt install.\n');
  });

  it('fails on an unclosed or stray marker', () => {
    assert.throws(() => renderNotes('{{MAC_UNSIGNED}}\na\n', { macUnsigned: true }), /MAC_UNSIGNED/);
    assert.throws(() => renderNotes('a\n{{/MAC_UNSIGNED}}\n', { macUnsigned: false }), /MAC_UNSIGNED/);
    assert.throws(
      () => renderNotes('{{MAC_UNSIGNED}}\n{{MAC_UNSIGNED}}\n{{/MAC_UNSIGNED}}\n', { macUnsigned: true }),
      /MAC_UNSIGNED/,
    );
  });
});

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
  it('keeps, removes, and repeats MAC_UNSIGNED blocks', () => {
    assert.equal(
      renderNotes(template, { macUnsigned: true }),
      '## Install\n\nUnsigned: run xattr.\n\nLinux: apt install.\n',
    );
    assert.equal(renderNotes(template, { macUnsigned: false }), '## Install\n\nLinux: apt install.\n');

    const multi = '{{MAC_UNSIGNED}}\na\n{{/MAC_UNSIGNED}}\nb\n{{MAC_UNSIGNED}}\nc\n{{/MAC_UNSIGNED}}\n';
    assert.equal(renderNotes(multi, { macUnsigned: true }), 'a\nb\nc\n');
    assert.equal(renderNotes(multi, { macUnsigned: false }), 'b\n');
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
    for (const body of [
      '{{MAC_UNSIGNED}}\na\n',
      'a\n{{/MAC_UNSIGNED}}\n',
      '{{MAC_UNSIGNED}}\n{{MAC_UNSIGNED}}\n{{/MAC_UNSIGNED}}\n',
    ]) {
      assert.throws(() => renderNotes(body, { macUnsigned: true }), /MAC_UNSIGNED/);
    }
  });
});

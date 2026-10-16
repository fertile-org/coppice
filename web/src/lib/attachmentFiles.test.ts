import { describe, expect, it } from 'vitest';
import {
  attachmentFailedMessage,
  attachmentRejection,
  attachmentTooLargeMessage,
  attachmentTypeMessage,
  MAX_ATTACHMENT_BYTES,
} from './attachmentFiles';

function file(name: string, type: string, size = 12): File {
  const blob = new File([new Uint8Array(size)], name, { type });
  return blob;
}

describe('attachment file messages', () => {
  it('rejects a file over the limit with the locked sentence', () => {
    const big = file('diagram.png', 'image/png', MAX_ATTACHMENT_BYTES + 1);
    expect(attachmentRejection(big)).toBe(attachmentTooLargeMessage(big));
    expect(attachmentRejection(big)).toBe(
      'diagram.png is too large. The limit is 10.0 MB.',
    );
  });

  it('rejects a disallowed type with the locked sentence', () => {
    const html = file('page.html', 'text/html');
    expect(attachmentRejection(html)).toBe(attachmentTypeMessage(html));
    expect(attachmentRejection(html)).toBe("Coppice can't attach html files.");
  });

  it('accepts png, markdown, and pdf', () => {
    expect(attachmentRejection(file('shot.png', 'image/png'))).toBeNull();
    expect(attachmentRejection(file('notes.md', 'text/markdown'))).toBeNull();
    expect(attachmentRejection(file('spec.pdf', 'application/pdf'))).toBeNull();
    expect(attachmentRejection(file('notes.txt', ''))).toBeNull();
  });

  it('uses the locked upload failure sentence', () => {
    expect(attachmentFailedMessage(file('shot.png', 'image/png'))).toBe(
      "Couldn't attach shot.png. Try again.",
    );
  });
});

export const CAPTION_MAX = 60;

export function assertCaption(text: string): void {
  if (text.trim().length === 0) {
    throw new Error("Caption is empty");
  }
  if (text.length > CAPTION_MAX) {
    throw new Error(
      `Caption is ${text.length} characters, the limit is ${CAPTION_MAX}: "${text}"`,
    );
  }
}

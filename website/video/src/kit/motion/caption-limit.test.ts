import { test } from "node:test";
import assert from "node:assert/strict";
import { assertCaption, CAPTION_MAX } from "./caption-limit.ts";

test("CAPTION_MAX is 60", () => {
  assert.equal(CAPTION_MAX, 60);
});

test("a 60-character caption passes", () => {
  assert.doesNotThrow(() => assertCaption("a".repeat(60)));
});

test("a 61-character caption throws naming both lengths", () => {
  assert.throws(() => assertCaption("a".repeat(61)), /61.*60|60.*61/);
});

test("an empty caption throws", () => {
  assert.throws(() => assertCaption(""));
});

test("a whitespace-only caption throws", () => {
  assert.throws(() => assertCaption("   "));
});

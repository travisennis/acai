import { strict as assert } from "node:assert/strict";
import { describe, it } from "node:test";
import {
  getListNumber,
  isMarkdown,
} from "../../source/terminal/markdown-utils.ts";

describe("isMarkdown", () => {
  it("should return true for markdown headings", () => {
    assert.strictEqual(isMarkdown("# Heading 1"), true);
    assert.strictEqual(isMarkdown("## Heading 2"), true);
    assert.strictEqual(isMarkdown("### Heading 3"), true);
    assert.strictEqual(isMarkdown("#### Heading 4"), true);
    assert.strictEqual(isMarkdown("##### Heading 5"), true);
    assert.strictEqual(isMarkdown("###### Heading 6"), true);
  });

  it("should return true for markdown bold text", () => {
    assert.strictEqual(isMarkdown("**bold text**"), true);
    assert.strictEqual(isMarkdown("__bold text__"), true);
  });

  it("should return true for markdown italic text", () => {
    assert.strictEqual(isMarkdown("*italic text*"), true);
    assert.strictEqual(isMarkdown("_italic text_"), true);
  });

  it("should return true for markdown code blocks", () => {
    assert.strictEqual(isMarkdown("`code`"), true);
    assert.strictEqual(isMarkdown("```code block```"), true);
  });

  it("should return true for markdown links", () => {
    assert.strictEqual(isMarkdown("(alt text](url)"), true);
  });

  it("should return true for markdown blockquotes", () => {
    assert.strictEqual(isMarkdown("> blockquote"), true);
  });

  it("should return true for markdown unordered lists", () => {
    assert.strictEqual(isMarkdown("- list item"), true);
    assert.strictEqual(isMarkdown("* list item"), true);
    assert.strictEqual(isMarkdown("+ list item"), true);
  });

  it("should return true for markdown ordered lists", () => {
    assert.strictEqual(isMarkdown("1. list item"), true);
  });

  it("should return true for markdown horizontal rules", () => {
    assert.strictEqual(isMarkdown("---"), true);
  });

  it("should return true for markdown images", () => {
    assert.strictEqual(isMarkdown("![alt text](url)"), true);
  });

  it("should return false for inline text with asterisk operators", () => {
    assert.strictEqual(isMarkdown("price is 3 * 4"), false);
  });

  it("should return false for plain text", () => {
    assert.strictEqual(isMarkdown("This is plain text."), false);
  });

  it("should return false for an empty string", () => {
    assert.strictEqual(isMarkdown(""), false);
  });

  it("should return true for mixed markdown content", () => {
    assert.strictEqual(
      isMarkdown("# Heading\nSome *italic* and **bold** text."),
      true,
    );
  });
});

describe("getListNumber", () => {
  it("should return arabic numerals for depth 0", () => {
    assert.strictEqual(getListNumber(0, 1), "1");
    assert.strictEqual(getListNumber(0, 10), "10");
    assert.strictEqual(getListNumber(0, 0), "0"); // Though practically, list numbers start from 1
  });

  it("should return arabic numerals for depth 1", () => {
    assert.strictEqual(getListNumber(1, 1), "1");
    assert.strictEqual(getListNumber(1, 5), "5");
  });

  it("should return lowercase letters for depth 2", () => {
    assert.strictEqual(getListNumber(2, 1), "a");
    assert.strictEqual(getListNumber(2, 26), "z");
    assert.strictEqual(getListNumber(2, 27), "aa");
    assert.strictEqual(getListNumber(2, 0), "");
  });

  it("should return lowercase Roman numerals for depth 3", () => {
    assert.strictEqual(getListNumber(3, 1), "i");
    assert.strictEqual(getListNumber(3, 4), "iv");
    assert.strictEqual(getListNumber(3, 9), "ix");
    assert.strictEqual(getListNumber(3, 10), "x");
    assert.strictEqual(getListNumber(3, 40), "xl");
    assert.strictEqual(getListNumber(3, 0), "");
  });

  it("should return arabic numerals for depth greater than 3 (default case)", () => {
    assert.strictEqual(getListNumber(4, 1), "1");
    assert.strictEqual(getListNumber(10, 5), "5");
  });

  it("should handle various numbers for depth 1 list (arabic)", () => {
    assert.strictEqual(getListNumber(1, 123), "123");
  });

  it("should handle various numbers for depth 2 list (letters)", () => {
    assert.strictEqual(getListNumber(2, 52), "az");
    assert.strictEqual(getListNumber(2, 702), "zz");
    assert.strictEqual(getListNumber(2, 703), "aaa");
  });

  it("should handle various numbers for depth 3 list (roman)", () => {
    assert.strictEqual(getListNumber(3, 49), "xlix");
    assert.strictEqual(getListNumber(3, 99), "xcix");
    assert.strictEqual(getListNumber(3, 499), "cdxcix");
    assert.strictEqual(getListNumber(3, 1994), "mcmxciv");
  });
});

const markdownHeaderRegex = /^#{1,6}\s/m;
const markdownBoldRegex = /(\*\*|__)(.*?)\1/;
const markdownItalicRegex = /(\*|_)(.*?)\1/;
const markdownCodeRegex = /`{1,3}[^`]+`{1,3}/;
const markdownLinkRegex = /\((.*?)\]\((.*?)\)/;
const markdownBlockquoteRegex = /^>\s/m;
const markdownUnorderedListRegex = /^(?:-|\*|\+)\s/m;
const markdownOrderedListRegex = /^\d+\.\s/m;
const markdownHorizontalRuleRegex = /^---$/m;
const markdownImageRegex = /!\[(.*?)\]\((.*?)\)/;

export function isMarkdown(input: string): boolean {
  // Simple heuristics: look for common markdown syntax
  const markdownPatterns = [
    markdownHeaderRegex, // headings
    markdownBoldRegex, // bold
    markdownItalicRegex, // italic
    markdownCodeRegex, // inline code or code block
    markdownLinkRegex, // links
    markdownBlockquoteRegex, // blockquote
    markdownUnorderedListRegex, // unordered list
    markdownOrderedListRegex, // ordered list
    markdownHorizontalRuleRegex, // horizontal rule
    markdownImageRegex, // images
  ];
  return markdownPatterns.some((pattern) => pattern.test(input));
}

function getDepth1ListNumber(i: number): string {
  if (i <= 0) {
    return "";
  }
  const letters: string[] = [];
  let num = i;
  while (num > 0) {
    num--; // Adjust for 0-indexing
    letters.unshift(String.fromCharCode(97 + (num % 26)));
    num = Math.floor(num / 26);
  }
  return letters.join("");
}

function getDepth2ListNumber(i: number): string {
  if (i <= 0) {
    return "";
  }
  const romanNumerals = [
    ["m", 1000],
    ["cm", 900],
    ["d", 500],
    ["cd", 400],
    ["c", 100],
    ["xc", 90],
    ["l", 50],
    ["xl", 40],
    ["x", 10],
    ["ix", 9],
    ["v", 5],
    ["iv", 4],
    ["i", 1],
  ] as const;

  let num = i;
  let result = "";
  for (const [roman, value] of romanNumerals) {
    while (num >= value) {
      result += roman;
      num -= value;
    }
  }
  return result;
}

export function getListNumber(
  listDepth: number,
  orderedListNumber: number,
): string {
  switch (listDepth) {
    case 0:
    case 1:
      return orderedListNumber.toString();
    case 2:
      return getDepth1ListNumber(orderedListNumber);
    case 3:
      return getDepth2ListNumber(orderedListNumber);
    default:
      return orderedListNumber.toString();
  }
}

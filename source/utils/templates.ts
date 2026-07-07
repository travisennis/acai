export function replaceArgumentPlaceholders(
  content: string,
  args: string[],
): string {
  const allArguments = args.join(" ");

  let replacementsMade = false;
  let result = content;

  for (let i = args.length - 1; i >= 0; i--) {
    const placeholder = `$${i + 1}`;
    if (result.includes(placeholder)) {
      result = result.replaceAll(placeholder, args[i]);
      replacementsMade = true;
    }
  }

  if (result.includes("$ARGUMENTS")) {
    result = result.replaceAll("$ARGUMENTS", allArguments);
    replacementsMade = true;
  }

  if (result.includes("{{INPUT}}")) {
    result = result.replaceAll("{{INPUT}}", allArguments);
    replacementsMade = true;
  }

  if (!replacementsMade && allArguments.trim().length > 0) {
    result += `\n\n${allArguments}`;
  }

  return result;
}

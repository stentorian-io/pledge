function isBreaking(message) {
  return /^[a-z]+(?:\([^\r\n)]+\))?!:/m.test(message)
    || /^BREAKING[ -]CHANGE:/m.test(message);
}

function validateMessage(message) {
  const subject = message.split(/\r?\n/, 1)[0] ?? "";
  const match = subject.match(/^(feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert)(!)?: (.+)$/);
  if (!match) return "use <type>: <description> without a component or scope.";
  const [, , breaking, description] = match;
  if (!/^[a-z]/.test(description)) return "start the description with a lowercase letter.";
  if (description.endsWith(".")) return "omit the final period from the subject.";
  if (description !== description.trim()) return "remove trailing whitespace from the subject.";
  if (breaking && !/^BREAKING CHANGE: \S.+/m.test(message)) {
    return "explain the compatibility change and migration in a BREAKING CHANGE: footer.";
  }
  if (/^BREAKING[ -]CHANGE:\s*$/m.test(message)) return "describe the breaking change after its footer.";
  if (/^\s*(co[- ]authored[- ]by|co[- ]developed[- ]by|signed[- ]off[- ]by|reviewed[- ]by|tested[- ]by|acked[- ]by|generated[- ]by|assisted[- ]by):/im.test(message)) {
    return "remove attribution trailers (co-author, sign-off, reviewed-by, or generated-by).";
  }
  if (/^\s*(?:🤖\s*)?(?:generated|written|assisted) (?:with|by) \[?(?:Claude|Codex|ChatGPT|OpenAI|Anthropic)\b/im.test(message)) {
    return "remove generated-by attribution from the commit body.";
  }
  return null;
}

module.exports = { isBreaking, validateMessage };

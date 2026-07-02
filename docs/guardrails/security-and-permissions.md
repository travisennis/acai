# Security And Permissions

## Scope

Read this guardrail for filesystem writes, shell execution, dynamic tools, web
access, approval prompts, sandbox behavior, path validation, secrets, auth
headers, and logging redaction.

## Compatibility Surfaces

- Bash and dynamic tool approval behavior.
- Filesystem boundary checks and path normalization.
- Web search/fetch provider usage and fallback behavior.
- Environment variable handling and secret redaction.
- User-visible permission prompts and denial behavior.

## Required Checks

- Add security-focused regression tests for boundary and denial cases.
- Exercise both allowed and rejected paths for filesystem/shell changes.
- Verify secrets are not logged, echoed, serialized, or committed.
- Security-sensitive durable decisions require ADR coverage through
  `docs/adr/index.md`.

## Common Failure Modes

- Expanding write/read access while fixing a convenience issue.
- Logging env vars, auth headers, command lines, or full tool payloads.
- Allowing path traversal through symlinks, relative paths, or `~` expansion.
- Letting dynamic tools bypass approval or workspace boundaries.

## Related Docs

- `docs/dynamic-tools.md`
- `docs/adr/003-tool-calling-interface.md`
- `docs/adr/009-dynamic-tool-loading.md`
- `source/utils/filesystem/security.ts`
- `test/utils/filesystem/security.test.ts`
- `test/tools/bash.test.ts`

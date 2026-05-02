# ADR 003: Settings Profiles

**Status:** Accepted  
**Date:** 2026-05-02

## Context

cake settings currently let users define named model provider configurations, a `default_model`, skill filters, and additional sandbox directories. These settings work well for stable defaults, but users often need to change agent behavior between tasks. For example, a quick implementation task may need a different default model and fewer skills than a careful review task.

Using CLI flags for every run is repetitive, and duplicating model provider configs inside every behavior variant would create drift. We need a way to switch behavior quickly while keeping model configuration centralized.

## Decision

We add named settings profiles selected with `cake --profile <name>`.

Profiles are defined in global or project `settings.toml` files:

```toml
[profiles.fast]
default_model = "deepseek"

[profiles.review.skills]
only = ["debugging-cake", "evaluating-cake"]

[profiles.expanded]
directories = ["../shared-libs"]
```

Profiles may configure:

1. `default_model`
2. `skills`
3. `directories`

Profiles may not define model provider configurations. All model configs remain in top-level `[[models]]` entries.

## Rationale

- **Fast behavior switching**: `--profile review` is easier and less error-prone than repeating several flags.
- **Centralized model configuration**: Profiles can select a default model, but do not duplicate base URLs, API key env vars, or provider-specific settings.
- **Project and user layering**: Global profiles can define personal defaults, while project profiles can refine behavior for a repository.
- **Clear precedence**: CLI flags remain the highest-precedence override, so one-off changes still work naturally.
- **Configuration reuse**: Profiles reuse existing settings concepts instead of introducing a separate config file or command system.

## Consequences

- **Positive**: Users can create task-oriented agent modes such as `fast`, `review`, or `docs`.
- **Positive**: Model configs remain a single source of truth.
- **Positive**: Profiles compose with existing global/project settings precedence.
- **Negative**: Settings merging becomes more complex because profile fields are partial overlays.
- **Negative**: Users must understand that `default_model` is profile-aware, but `[[models]]` is not.

## Alternatives Considered

- **Configure models per profile**: Rejected because it duplicates provider details and increases the risk of stale model config.
- **Use only CLI aliases or shell aliases**: Rejected because aliases are hard to share per project and do not compose with `settings.toml`.
- **Add separate profile files**: Rejected because it fragments configuration and complicates discovery.
- **Replace top-level settings when a profile is selected**: Rejected because profiles should be lightweight overlays, not full copies of settings.

## References

- `docs/design-docs/settings.md` - Settings and profile documentation
- `docs/design-docs/cli.md` - CLI flag documentation
- `src/config/settings.rs` - Profile parsing, validation, and merge behavior
- `src/main.rs` - `--profile` CLI integration

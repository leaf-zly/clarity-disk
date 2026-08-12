# Clarity Disk engineering rules

## Build and release

- Do not build Windows installers or portable release archives on developer machines.
- Local verification is limited to formatting, linting, type checking, tests, frontend builds, and Rust checks.
- Produce distributable binaries only through the reviewed GitHub Actions release workflow.
- Keep release workflows Windows-only until another platform is explicitly supported.

## Architecture and safety

- Keep UI code, domain rules, Windows platform integration, and privileged operations in separate modules.
- Destructive filesystem and partition operations require an immutable operation plan, a preview, explicit confirmation, and an audit record.
- Prefer documented Windows APIs or official vendor cleanup commands over direct deletion of system-managed files.
- Never introduce a privileged command that accepts arbitrary shell text or an unchecked filesystem path.

## Quality

- Add JSDoc to exported TypeScript functions, component Props/Emits, composables, and non-trivial exported types.
- Add Rustdoc to public Rust APIs and explain safety invariants for privileged operations.
- Update comments and documentation in the same change as the behavior they describe.
- New behavior requires tests at the lowest useful layer.

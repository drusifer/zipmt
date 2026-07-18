# Bob Context

## Recent Decisions

- Judge uses pinned Tracegate 0.1.6 for Claude/Codex native trace normalization.
- Judge retains a thin mapping layer and its project-specific anti-pattern/TES evaluation rules.
- Judge Python dependencies live in `.judge-venv`, created only through `make judge-tools-install`.

## Key Findings

- Tracegate supports both Claude Code and Codex CLI, including policy checks and CI exit codes.
- Codex code-mode calls remain wrapped as `custom_tool_call(name=exec)` in Tracegate 0.1.6; Judge unwraps the preserved raw `tools.*` payload so command-level rules remain effective.

## Important Notes

The focused parser suite covers Claude tool calls, Codex function calls, Codex code-mode shell calls, and Codex patch paths.

---
*Last updated: 2026-07-18T18:27:00-04:00*

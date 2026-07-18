# Judge Defects — Tool and Skill Use Session

## JUDGE-001: Active Codex session missed by today's date

- Reproduction: `make judge-trace DATE=2026-07-18 SOURCE=codex`
- Actual: no sessions found.
- Expected: an option to select the active/latest matching project session even when it began on a previous date.

## JUDGE-002: Codex skill reloads are invisible

- Actual: Codex filesystem reads of `SKILL.md` remain Bash events.
- Expected: Judge derives canonical Skill events so `AP-SKILL-RELOAD` can operate across supported harnesses.

## JUDGE-003: Shell rule false positives

**Status: Resolved 2026-07-18**

- `AP-MAKE-BYPASS` scans quoted chat message content.
- `AP-MAKE-PIPE` treats `||` as a pipeline.
- `AP-VIA-GREP` scans across unrelated semicolon-delimited command segments.

Expected: classify executable shell segments rather than arbitrary text spanning a compound command.

Resolution: Judge now tokenizes shell input with quote-aware `shlex`, identifies
the executable at each control-operator boundary, distinguishes `|` from `||`,
and confines Via heuristics to the arguments of the `rg`/`grep` segment.
Full-session rerun: eight false flags removed; the one confirmed Via violation
remains.

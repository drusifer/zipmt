# Judge False-Positive Fix

## Outcome

Replaced whole-string shell regex classification with quote-aware executable
segment analysis.

## Coverage

- Tool names inside `make chat MSG="..."` are ignored.
- `||` is not treated as a Make output pipe.
- Real `make test | tail` remains flagged.
- Via matching cannot cross a semicolon into another command.
- Real symbol-oriented `rg` remains flagged.
- Direct pytest execution remains flagged.

## Verification

- `make judge-trace-test V=-vvv`: 10/10 passed.
- Full active Codex session: 822 calls, one flag, one session.

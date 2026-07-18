# Judge Tool-Call Efficiency Rubric

## Outcome

Judge now applies MLflow `ToolCallEfficiency`'s core criterion—efficient tool
usage without redundant or unnecessary calls—using deterministic Tracegate
event checks rather than an LLM judge.

## Checks

- Exact tool and argument duplicates within five calls
- Immediate unchanged retries
- Same test rerun without a code edit
- Same search rerun without an edit or scope change
- Existing repeated file-read detection
- Stateful polling exemptions for `write_stdin` and `wait_agent`

## Verification

- Focused tests: 16/16 passed.
- Full session: 836 calls, two total flags.
- Efficiency verdict: NO, with one confirmed unchanged retest.
- The other flag is the previously confirmed Via bypass.

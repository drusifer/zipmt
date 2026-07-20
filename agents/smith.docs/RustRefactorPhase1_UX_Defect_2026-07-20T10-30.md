# Phase 1 UX Defect — Bounded Work Panels

## Evidence

The user ran both Split and Stream TUI modes at 80x22. Both renderings otherwise
looked good.

## Defect

`*user bug CMD: run both TUI modes at 80x22 | EXPECTED: reach every slice and
worker in bounded panels | ACTUAL: the slice and worker areas do not provide
consistent, discoverable scrolling for off-screen entries | UX ISSUE: hidden
work cannot be inspected`

## HCI basis

- #1 Visibility of system status: off-screen work state is unavailable.
- #3 User control and freedom: the user cannot navigate the full worker list.
- #4 Consistency and standards: equivalent bounded work lists must use the same
  navigation model.
- #6 Recognition rather than recall: the UI must expose a concise scroll/range
  cue.

## Required correction

- PageUp/PageDown pages the active mode's work list.
- Mouse wheel over the left work panel scrolls that list.
- Panel title shows the visible range and a concise navigation cue.
- Offsets clamp safely when the viewport or item count changes.
- Behavior is covered at 80x22 and 120x30.

## Gate

Phase 1 UX: **Rejected pending correction and retest**.

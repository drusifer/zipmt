#!/usr/bin/env python3
"""
trace_annotate.py — Annotated tool-use extractor for Codex CLI and Claude Code.

Usage:
    python agents/tools/trace_annotate.py [--date YYYY-MM-DD] [--out FILE]
                                           [--format html|md] [--rules FILE]
                                           [--no-via] [--project DIR]
                                           [--source auto|codex|claude]

Defaults:
    --date      yesterday
    --out       agents/trin.docs/judge_tool_trace.html  (or .md if --format md)
    --format    html
    --rules     agents/tools/trace_rules.json   (auto-loaded if present)
    --project   project working directory (default: CWD)
    --source    Codex traces first, then Claude fallback

Anti-patterns detected:
    AP-SKILL-RELOAD    Same Skill invoked more than once in a session
    AP-MAKE-BYPASS     Bash runs pytest/ruff/python/.venv directly instead of make
    AP-MAKE-PIPE       Bash pipes make output (violates make skill rule)
    AP-VIA-GREP        Grep/Glob used for symbol/import/function lookups
    AP-VIA-READ        Read used on source files never subsequently edited
    AP-DUP-READ        Same file Read 3+ times in one session
    AP-RAW-VENV        Bash calls .venv/bin/<tool> directly
"""

import argparse
import html as html_lib
import json
import os
import re
import shlex
import subprocess
import sys
from collections import Counter
from datetime import date, timedelta
from pathlib import Path

from jinja2 import Template


# ---------------------------------------------------------------------------
# Built-in rules (seeded into trace_rules.json on first run)
# ---------------------------------------------------------------------------

BUILTIN_RULES: dict[str, dict] = {
    'AP-SKILL-RELOAD': {
        'description': 'Same Skill invoked >1× in a session — wastes full SKILL.md reload (~6k tokens each).',
        'fix': 'After the first Skill load, call sub-commands via Bash directly. Do NOT re-invoke Skill(make,...) or Skill(chat,...) within the same session.',
        'color': '#6366f1',
    },
    'AP-MAKE-BYPASS': {
        'description': 'Bash runs pytest/ruff/pylint/mypy directly — bypasses make targets.',
        'fix': 'Always use make <target>. Output is captured to build/build.out by the Makefile.',
        'color': '#ef4444',
    },
    'AP-RAW-VENV': {
        'description': 'Bash calls .venv/bin/<tool> directly.',
        'fix': 'Use make <target> — never reference .venv/bin directly. If no make target exists, add one to Makefile.prj.',
        'color': '#dc2626',
    },
    'AP-MAKE-PIPE': {
        'description': 'Bash pipes make output — mkf already captures to build/build.out.',
        'fix': 'Run make <target>, then tail -n 30 build/build.out. Or use V=-vv to see failures live. Never pipe.',
        'color': '#f97316',
    },
    'AP-VIA-GREP': {
        'description': 'Grep/Glob used for symbol/import/function lookup when via is enabled.',
        'fix': 'Use mcp__via__via_query for symbol lookups. Reserve grep for free-text content search only.',
        'color': '#8b5cf6',
    },
    'AP-VIA-READ': {
        'description': 'Read on a source file with no subsequent Edit/Write — likely symbol hunting.',
        'fix': 'Use mcp__via__via_query to locate symbols. Read is only needed before editing or full-file review.',
        'color': '#a855f7',
    },
    'AP-DUP-READ': {
        'description': 'Same file Read 3+ times in one session without the file changing.',
        'fix': 'Read once, keep excerpt in context. Re-Read only to verify after an edit.',
        'color': '#eab308',
    },
    'AP-DUP-TOOL': {
        'description': 'Same tool and arguments repeated within five calls without an intervening edit.',
        'fix': 'Reuse the prior result or explain what new information makes the repeated call necessary.',
        'color': '#fb7185',
    },
    'AP-UNCHANGED-RETRY': {
        'description': 'A tool call was immediately retried with identical arguments.',
        'fix': 'Inspect the failure, change the inputs or strategy, and cap unchanged retries.',
        'color': '#f43f5e',
    },
    'AP-RETEST-NO-CHANGE': {
        'description': 'The same test target was rerun without an intervening code edit.',
        'fix': 'Do not rerun an unchanged test unless external state changed; record that reason when it did.',
        'color': '#fb923c',
    },
    'AP-SEARCH-LOOP': {
        'description': 'The same search was repeated without an intervening edit or scope change.',
        'fix': 'Reuse the earlier result or change the query/scope before searching again.',
        'color': '#c084fc',
    },
}

DEFAULT_RULES_PATH = Path('agents/tools/trace_rules.json')

TOOL_COLORS = {
    'Bash': '#64748b',
    'Read': '#3b82f6',
    'Edit': '#10b981',
    'Write': '#f59e0b',
    'Skill': '#8b5cf6',
    'Agent': '#ec4899',
    'mcp__via__via_query': '#06b6d4',
    'mcp__via__via_ask': '#06b6d4',
    'TaskCreate': '#84cc16',
    'TaskUpdate': '#84cc16',
    'TaskGet': '#84cc16',
    'TaskList': '#84cc16',
    'WebSearch': '#f43f5e',
    'WebFetch': '#f43f5e',
}


# ---------------------------------------------------------------------------
# Rules management
# ---------------------------------------------------------------------------

def load_rules(path: Path) -> dict[str, dict]:
    rules = dict(BUILTIN_RULES)
    if path.exists():
        with open(path) as f:
            saved = json.load(f)
        rules.update(saved)
        # Backfill color if missing (rules.json written before colors were added)
        for code, rule in rules.items():
            if 'color' not in rule and code in BUILTIN_RULES:
                rule['color'] = BUILTIN_RULES[code]['color']
    else:
        path.parent.mkdir(parents=True, exist_ok=True)
        with open(path, 'w') as f:
            json.dump({k: {kk: vv for kk, vv in v.items() if kk != 'color'}
                       for k, v in BUILTIN_RULES.items()}, f, indent=2)
        print(f'Created rules file: {path}')
    return rules


# ---------------------------------------------------------------------------
# Detection
# ---------------------------------------------------------------------------

MAKE_BYPASS_TOOLS = {
    'pytest', 'ruff', 'pylint', 'mypy', 'black', 'isort', 'coverage', 'py.test',
}
# Only flag `make <target> ... |` when <target> is actually routed through mkf.py
# (captured to build/build.out, per the Makefile's interception layer). Targets
# excluded from mkf capture (chat, help, install_bob, update_bob, pull_bob,
# clean_bob) have no build.out equivalent to tail instead, so piping their
# output isn't the anti-pattern this rule targets.
MKF_EXCLUDED_TARGETS = {'help', 'chat', 'install_bob', 'update_bob', 'pull_bob', 'clean_bob'}
VIA_SYMBOL_GREP_RE = re.compile(
    r'(?:def |class |import |from |__init__|__call__|->|@\w+)',
    re.IGNORECASE
)
SOURCE_EXTENSIONS = {'.py', '.ts', '.tsx', '.js', '.jsx', '.go', '.rs', '.rb'}
EFFICIENCY_RULES = {
    'AP-DUP-TOOL', 'AP-UNCHANGED-RETRY', 'AP-RETEST-NO-CHANGE',
    'AP-SEARCH-LOOP', 'AP-DUP-READ',
}
EFFICIENCY_EXEMPT_TOOLS = {'write_stdin', 'wait_agent'}


def _shell_segments(cmd: str) -> list[tuple[list[str], str | None]]:
    """Return quote-aware command segments and the operator following each."""
    lexer = shlex.shlex(cmd, posix=True, punctuation_chars=';&|')
    lexer.whitespace_split = True
    lexer.commenters = ''
    try:
        tokens = list(lexer)
    except ValueError:
        return [([cmd], None)]

    segments: list[tuple[list[str], str | None]] = []
    current: list[str] = []
    for token in tokens:
        if token in {';', '&&', '||', '|', '&'}:
            if current:
                segments.append((current, token))
                current = []
        else:
            current.append(token)
    if current:
        segments.append((current, None))
    return segments


def _executable(tokens: list[str]) -> tuple[str, int]:
    index = 0
    while index < len(tokens) and re.match(r'^[A-Za-z_]\w*=.*$', tokens[index]):
        index += 1
    while index < len(tokens) and tokens[index] in {'command', 'env', 'sudo'}:
        index += 1
    if index >= len(tokens):
        return '', index
    return tokens[index], index


def classify_bash(cmd: str) -> list[str]:
    flags: list[str] = []
    for tokens, following_operator in _shell_segments(cmd):
        executable, index = _executable(tokens)
        tool = Path(executable).name
        if tool in MAKE_BYPASS_TOOLS and 'AP-MAKE-BYPASS' not in flags:
            flags.append('AP-MAKE-BYPASS')
        if re.search(r'(?:^|/)(?:\.venv|venv)/bin/[^/]+$', executable):
            if 'AP-RAW-VENV' not in flags:
                flags.append('AP-RAW-VENV')
        if tool == 'make' and following_operator == '|':
            target_index = index + 1
            if target_index < len(tokens) and tokens[target_index].startswith('MKF_ACTIVE='):
                target_index += 1
            target = tokens[target_index] if target_index < len(tokens) else ''
            if target not in MKF_EXCLUDED_TARGETS and 'AP-MAKE-PIPE' not in flags:
                flags.append('AP-MAKE-PIPE')
        if tool in {'rg', 'grep'}:
            arguments = ' '.join(tokens[index + 1:])
            if VIA_SYMBOL_GREP_RE.search(arguments) and 'AP-VIA-GREP' not in flags:
                flags.append('AP-VIA-GREP')
    return flags


# ---------------------------------------------------------------------------
# Parsing
# ---------------------------------------------------------------------------

def _json_dict(value) -> dict:
    if isinstance(value, dict):
        return value
    if isinstance(value, str):
        try:
            parsed = json.loads(value)
            return parsed if isinstance(parsed, dict) else {}
        except json.JSONDecodeError:
            return {}
    return {}


def _canonical_tool(name: str, inp: dict) -> tuple[str, dict]:
    short = name.rsplit('.', 1)[-1]
    aliases = {
        'exec_command': 'Bash', 'read_file': 'Read', 'apply_patch': 'Edit',
        'spawn_agent': 'Agent', 'search_query': 'WebSearch', 'open': 'WebFetch',
    }
    canonical = aliases.get(short, name)
    normalized = dict(inp)
    if canonical == 'Bash' and 'command' not in normalized:
        normalized['command'] = normalized.get('cmd', '')
    return canonical, normalized


def _codex_custom_calls(payload: dict, timestamp: str) -> list[dict]:
    """Extract tool calls from Codex code-mode custom_tool_call JavaScript."""
    script = str(payload.get('input', ''))
    calls = []
    for match in re.finditer(r'\btools\.([A-Za-z_]\w*)\s*\(', script):
        tool = match.group(1)
        inp: dict = {}
        if tool == 'exec_command':
            cmd = re.search(r'\bcmd\s*:\s*("(?:\\.|[^"\\])*")', script[match.end():])
            if cmd:
                try:
                    inp['cmd'] = json.loads(cmd.group(1))
                except json.JSONDecodeError:
                    pass
        elif tool == 'apply_patch':
            patch_literal = re.search(
                r'\b(?:const|let)\s+patch\s*=\s*("(?:\\.|[^"\\])*")', script)
            if patch_literal:
                try:
                    patch = json.loads(patch_literal.group(1))
                    paths = re.findall(
                        r'^\*\*\* (?:Add|Update|Delete) File: (.+)$',
                        patch, re.MULTILINE)
                    inp['file_path'] = ', '.join(paths)
                except json.JSONDecodeError:
                    pass
        name, inp = _canonical_tool(tool, inp)
        calls.append({
            'name': name, 'input': inp,
            'id': payload.get('call_id', payload.get('id', '')), 'ts': timestamp,
        })
    if not calls:
        name, inp = _canonical_tool(payload.get('name', 'custom_tool_call'), {})
        calls.append({
            'name': name, 'input': inp,
            'id': payload.get('call_id', payload.get('id', '')), 'ts': timestamp,
        })
    return calls


def parse_tracegate_events(trace: dict) -> list[dict]:
    """Map Tracegate's normalized events into Judge's compact tool-call model."""
    events = []
    for event in trace.get('run', {}).get('events', []):
        raw = event.get('raw') or {}
        timestamp = str(event.get('timestamp') or '')
        if raw.get('type') == 'function_call':
            name, inp = _canonical_tool(
                raw.get('name', ''), _json_dict(raw.get('arguments', {})))
            events.append({
                'name': name, 'input': inp,
                'id': raw.get('call_id', raw.get('id', '')), 'ts': timestamp,
            })
        elif raw.get('type') == 'custom_tool_call':
            events.extend(_codex_custom_calls(raw, timestamp))
        elif raw.get('type') == 'tool_use':
            name, inp = _canonical_tool(raw.get('name', ''), _json_dict(raw.get('input', {})))
            events.append({
                'name': name, 'input': inp, 'id': raw.get('id', ''), 'ts': timestamp,
            })
        elif event.get('command'):
            events.append({
                'name': 'Bash', 'input': {'command': event['command']},
                'id': '', 'ts': timestamp,
            })
        elif event.get('type') in ('file_read', 'read_file'):
            events.append({
                'name': 'Read', 'input': {'file_path': event.get('path', '')},
                'id': '', 'ts': timestamp,
            })
        elif event.get('type') in ('file_edit', 'file_write', 'write_file'):
            events.append({
                'name': 'Edit', 'input': {'file_path': event.get('path', '')},
                'id': '', 'ts': timestamp,
            })
        elif event.get('type') == 'tool_call':
            name, inp = _canonical_tool(
                str(event.get('tool') or event.get('category') or ''), _json_dict(event.get('input', {})))
            events.append({'name': name, 'input': inp, 'id': '', 'ts': timestamp})
    return events


def parse_session(path: Path, source: str) -> list[dict]:
    """Normalize a native CLI trace with Tracegate, then extract tool calls."""
    command = [
        sys.executable, '-m', 'tracegate.cli', 'inspect',
        '--format', source, '--json', '--include-raw', str(path),
    ]
    try:
        result = subprocess.run(command, check=True, capture_output=True, text=True)
    except FileNotFoundError as exc:
        raise RuntimeError('Tracegate is not installed; run: make judge-tools-install') from exc
    except subprocess.CalledProcessError as exc:
        raise RuntimeError(f'Tracegate failed for {path}: {exc.stderr.strip()}') from exc
    return parse_tracegate_events(json.loads(result.stdout))


def summarize_input(name: str, inp: dict) -> str:
    if name == 'Bash':
        cmd = inp.get('command', '')
        s = cmd[:200] if len(cmd) <= 200 else cmd[:197] + '...'
        return s
    if name == 'Read':
        p = inp.get('file_path', '')
        suffix = ''
        if inp.get('offset'):
            suffix += f' [offset={inp["offset"]}]'
        if inp.get('limit'):
            suffix += f' [limit={inp["limit"]}]'
        return p + suffix
    if name == 'Edit':
        old = str(inp.get('old_string', ''))[:80]
        return inp.get('file_path', '') + ' | ' + repr(old)
    if name == 'Write':
        return inp.get('file_path', '')
    if name == 'Skill':
        return f"skill={inp.get('skill','')}  args={str(inp.get('args',''))[:100]}"
    if name in ('mcp__via__via_query', 'mcp__via__via_ask'):
        return f"args={inp.get('args', '')}"
    if name == 'Agent':
        return str(inp.get('description', ''))[:100]
    if name in ('TaskCreate', 'TaskUpdate', 'TaskGet', 'TaskList', 'TaskStop'):
        return str(inp)[:120]
    if name == 'WebSearch':
        return str(inp.get('query', ''))[:100]
    if name == 'WebFetch':
        return str(inp.get('url', ''))[:120]
    return str(inp)[:140]


def _paths_edited(events: list[dict]) -> set[str]:
    edited = set()
    for ev in events:
        if ev['name'] in ('Edit', 'Write'):
            p = ev['input'].get('file_path', '')
            if p:
                edited.add(p)
    return edited


def _normalized_signature(name: str, inp: dict) -> str:
    return f'{name}:{json.dumps(inp, sort_keys=True, separators=(",", ":"), default=str)}'


def _bash_activity(command: str) -> str | None:
    """Classify commands used by deterministic efficiency checks."""
    for tokens, _ in _shell_segments(command):
        executable, index = _executable(tokens)
        tool = Path(executable).name
        arguments = tokens[index + 1:]
        if tool == 'make':
            while arguments and arguments[0].startswith(('MKF_ACTIVE=', 'V=')):
                arguments.pop(0)
            target = arguments[0] if arguments else ''
            if target.startswith('test') or target in {
                    'rust-clippy', 'rust-quality', 'rust-format-check', 'judge-trace-test'}:
                return 'test'
        if tool in {'rg', 'grep', 'find'}:
            return 'search'
        if tool == 'via':
            return 'search'
    return None


def annotate_events(events: list[dict], rules: dict, no_via: bool) -> list[dict]:
    """Return list of annotated event dicts for template rendering."""
    skill_seen: Counter = Counter()
    edited_paths = _paths_edited(events)
    # Per-path edit "generation" — bumped on every Edit/Write to that path, so a
    # Read at a given offset only counts as a duplicate of an earlier Read at
    # the same offset if no edit landed on the file in between.
    edit_generation: Counter = Counter()
    read_sig_seen: Counter = Counter()
    last_signature: str | None = None
    recent_signatures: dict[str, tuple[int, int]] = {}
    activity_seen: dict[tuple[str, str, int], int] = {}
    mutation_generation = 0
    skill_reload_allowed = set(rules.get('AP-SKILL-RELOAD', {}).get('multi_call_allowed', []))
    annotated = []

    for seq, ev in enumerate(events, 1):
        name = ev['name']
        inp = ev['input']
        flags: list[str] = []
        signature = _normalized_signature(name, inp)
        activity = _bash_activity(inp.get('command', '')) if name == 'Bash' else None

        if name == 'Bash':
            flags = classify_bash(inp.get('command', ''))
        elif name == 'Read':
            path = inp.get('file_path', '')
            sig = (path, inp.get('offset'), inp.get('limit'), edit_generation[path])
            read_sig_seen[sig] += 1
            if (Path(path).suffix in SOURCE_EXTENSIONS
                    and path not in edited_paths
                    and not no_via):
                flags.append('AP-VIA-READ')
            if read_sig_seen[sig] >= 3:
                flags.append('AP-DUP-READ')
        elif name in ('Edit', 'Write'):
            path = inp.get('file_path', '')
            if path:
                edit_generation[path] += 1
            mutation_generation += 1
        elif name == 'Skill':
            skill = inp.get('skill', '')
            skill_seen[skill] += 1
            if skill_seen[skill] > 1 and skill not in skill_reload_allowed:
                flags.append('AP-SKILL-RELOAD')

        if activity:
            activity_key = (activity, signature, mutation_generation)
            if activity_key in activity_seen:
                flags.append('AP-RETEST-NO-CHANGE' if activity == 'test' else 'AP-SEARCH-LOOP')
            activity_seen[activity_key] = seq
        elif name not in {'Read', 'Edit', 'Write', 'Skill', *EFFICIENCY_EXEMPT_TOOLS}:
            if signature == last_signature:
                flags.append('AP-UNCHANGED-RETRY')
            elif signature in recent_signatures:
                prior_seq, prior_generation = recent_signatures[signature]
                if seq - prior_seq <= 5 and prior_generation == mutation_generation:
                    flags.append('AP-DUP-TOOL')

        recent_signatures[signature] = (seq, mutation_generation)
        last_signature = signature

        if no_via:
            flags = [f for f in flags if 'VIA' not in f]

        annotated.append({
            'seq': seq,
            'name': name,
            'summary': summarize_input(name, inp),
            'flags': flags,
            'color': TOOL_COLORS.get(name, '#94a3b8'),
        })

    return annotated


# ---------------------------------------------------------------------------
# HTML template
# ---------------------------------------------------------------------------

HTML_TEMPLATE = r"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Tool-Use Trace — {{ project }} {{ target_date }}</title>
<style>
  :root {
    --bg: #0f172a; --surface: #1e293b; --surface2: #263244;
    --border: #334155; --text: #e2e8f0; --muted: #94a3b8;
    --clean-row: #1e293b; --flagged-row: #2d1f0e;
  }
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { background: var(--bg); color: var(--text); font: 13px/1.5 'JetBrains Mono', 'Fira Code', monospace; }
  a { color: #60a5fa; }

  /* Header */
  .header { padding: 20px 24px 16px; border-bottom: 1px solid var(--border); background: var(--surface); }
  .header h1 { font-size: 18px; font-weight: 700; margin-bottom: 6px; }
  .header .meta { color: var(--muted); font-size: 12px; }
  .stats { display: flex; gap: 24px; margin-top: 12px; flex-wrap: wrap; }
  .stat { display: flex; flex-direction: column; }
  .stat .val { font-size: 22px; font-weight: 700; }
  .stat .lbl { font-size: 11px; color: var(--muted); text-transform: uppercase; letter-spacing: .05em; }

  /* Filters */
  .filters { padding: 12px 24px; border-bottom: 1px solid var(--border); background: var(--surface2);
             display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }
  .filters span { color: var(--muted); font-size: 11px; text-transform: uppercase; letter-spacing: .05em; margin-right: 4px; }
  .filter-btn { border: none; border-radius: 4px; padding: 3px 10px; font: inherit;
                font-size: 11px; cursor: pointer; transition: opacity .15s; }
  .filter-btn.active { opacity: 1; }
  .filter-btn.inactive { opacity: 0.35; }
  .filter-btn.all-btn { background: #334155; color: #e2e8f0; }

  /* Sessions */
  .sessions { padding: 16px 24px; display: flex; flex-direction: column; gap: 12px; }
  details { border: 1px solid var(--border); border-radius: 8px; overflow: hidden; }
  details[open] summary { border-bottom: 1px solid var(--border); }
  summary { padding: 10px 16px; cursor: pointer; background: var(--surface2); list-style: none;
            display: flex; align-items: center; gap: 12px; user-select: none; }
  summary::-webkit-details-marker { display: none; }
  summary .chevron { color: var(--muted); transition: transform .2s; font-size: 10px; }
  details[open] summary .chevron { transform: rotate(90deg); }
  summary .sid { font-weight: 700; }
  summary .smeta { color: var(--muted); font-size: 11px; }
  summary .sflag { margin-left: auto; display: flex; gap: 6px; align-items: center; }

  /* Tool call table */
  table { width: 100%; border-collapse: collapse; font-size: 12px; }
  tr.clean { background: var(--clean-row); }
  tr.flagged { background: var(--flagged-row); }
  tr:hover td { filter: brightness(1.12); }
  td { padding: 4px 8px; border-bottom: 1px solid #1a2535; vertical-align: top; }
  td.seq { color: var(--muted); width: 44px; text-align: right; white-space: nowrap; padding-right: 12px; }
  td.tool { width: 160px; white-space: nowrap; }
  td.summary { word-break: break-all; }
  td.badges { width: 220px; }

  /* Tool name chip */
  .tool-chip { display: inline-block; padding: 1px 7px; border-radius: 3px; font-weight: 600;
               font-size: 11px; color: #fff; }

  /* AP badge */
  .badge { display: inline-block; padding: 2px 6px; border-radius: 3px; font-size: 10px;
           font-weight: 700; color: #fff; cursor: help; position: relative; margin: 1px 2px 1px 0;
           white-space: nowrap; }
  .badge .tooltip { display: none; position: absolute; z-index: 100; bottom: calc(100% + 6px); left: 0;
                    background: #0f172a; border: 1px solid #475569; border-radius: 6px;
                    padding: 10px 12px; width: 320px; font-size: 11px; line-height: 1.5;
                    font-weight: 400; box-shadow: 0 8px 24px rgba(0,0,0,.5); }
  .badge:hover .tooltip { display: block; }
  .tooltip .t-code { font-weight: 700; margin-bottom: 4px; }
  .tooltip .t-desc { color: #94a3b8; margin-bottom: 6px; }
  .tooltip .t-fix-lbl { color: #64748b; font-size: 10px; text-transform: uppercase; letter-spacing: .04em; }
  .tooltip .t-fix { color: #e2e8f0; margin-top: 2px; }

  /* Summary table */
  .summary-section { padding: 0 24px 24px; }
  .summary-section h2 { font-size: 14px; margin-bottom: 12px; color: var(--muted);
                         text-transform: uppercase; letter-spacing: .06em; }
  .sum-table { border-collapse: collapse; width: 100%; max-width: 700px; }
  .sum-table th { text-align: left; padding: 6px 12px; font-size: 11px; color: var(--muted);
                   text-transform: uppercase; letter-spacing: .05em; border-bottom: 1px solid var(--border); }
  .sum-table td { padding: 6px 12px; border-bottom: 1px solid #1e293b; font-size: 12px; }
  .sum-table .count { font-weight: 700; font-size: 14px; }

  /* Hidden rows for filtering */
  tr.hidden { display: none; }
</style>
</head>
<body>

<div class="header">
  <h1>Tool-Use Trace — {{ project }}</h1>
  <div class="meta">{{ target_date }} &nbsp;·&nbsp; {{ sessions|length }} sessions &nbsp;·&nbsp;
    Rules: {{ rules_path }}</div>
  <div class="stats">
    <div class="stat"><span class="val">{{ total_calls }}</span><span class="lbl">Tool Calls</span></div>
    <div class="stat"><span class="val">{{ total_flags }}</span><span class="lbl">Flags</span></div>
    <div class="stat"><span class="val">{{ "%.1f"|format(total_flags / total_calls * 100 if total_calls else 0) }}%</span><span class="lbl">Flag Rate</span></div>
    <div class="stat"><span class="val">{{ sessions|length }}</span><span class="lbl">Sessions</span></div>
    <div class="stat"><span class="val">{{ efficiency_verdict }}</span><span class="lbl">Tool Efficiency</span></div>
  </div>
</div>

<div class="filters">
  <span>Filter:</span>
  <button class="filter-btn all-btn active" onclick="filterAll(this)">Show All</button>
  {% for code, rule in rules.items() %}
  <button class="filter-btn active" data-ap="{{ code }}"
          style="background:{{ rule.color }}; color:#fff;"
          onclick="filterAP(this, '{{ code }}')">{{ code }}</button>
  {% endfor %}
</div>

<div class="sessions">
{% for s in sessions %}
<details open>
  <summary>
    <span class="chevron">▶</span>
    <span class="sid">{{ s.sid }}</span>
    <span class="smeta">{{ s.time }} &nbsp;·&nbsp; {{ s.events|length }} calls &nbsp;·&nbsp; {{ s.flag_count }} flags</span>
    <span class="sflag">
      {% for code, cnt in s.ap_counts.items() %}
      <span class="badge" style="background:{{ rules[code].color if code in rules else '#64748b' }}">
        {{ code }} ×{{ cnt }}
      </span>
      {% endfor %}
    </span>
  </summary>
  <table>
    {% for ev in s.events %}
    <tr class="{{ 'flagged' if ev.flags else 'clean' }}{% for f in ev.flags %} ap-{{ f }}{% endfor %}"
        data-flags="{{ ev.flags|join(' ') }}">
      <td class="seq">{{ ev.seq }}</td>
      <td class="tool">
        <span class="tool-chip" style="background:{{ ev.color }}">{{ ev.name }}</span>
      </td>
      <td class="summary">{{ ev.summary | e }}</td>
      <td class="badges">
        {% for f in ev.flags %}
        {% set rule = rules.get(f, {}) %}
        <span class="badge" style="background:{{ rule.get('color','#64748b') }}">
          {{ f }}
          <span class="tooltip">
            <div class="t-code">{{ f }}</div>
            <div class="t-desc">{{ rule.get('description','') }}</div>
            <div class="t-fix-lbl">Fix</div>
            <div class="t-fix">{{ rule.get('fix','') }}</div>
          </span>
        </span>
        {% endfor %}
      </td>
    </tr>
    {% endfor %}
  </table>
</details>
{% endfor %}
</div>

<div class="summary-section">
  <h2>Summary</h2>
  <table class="sum-table">
    <tr><th>Anti-Pattern</th><th>Count</th><th>Fix</th></tr>
    {% for code, cnt in flag_counts %}
    {% set rule = rules.get(code, {}) %}
    <tr>
      <td><span class="badge" style="background:{{ rule.get('color','#64748b') }}">{{ code }}</span></td>
      <td class="count">{{ cnt }}</td>
      <td style="color:#94a3b8">{{ rule.get('fix','') }}</td>
    </tr>
    {% endfor %}
  </table>
</div>

<script>
const ALL_APS = {{ ap_codes | tojson }};
let activeFilters = new Set(ALL_APS);

function updateRows() {
  document.querySelectorAll('tr[data-flags]').forEach(row => {
    const flags = row.dataset.flags ? row.dataset.flags.split(' ').filter(Boolean) : [];
    if (flags.length === 0) {
      row.classList.remove('hidden');
      return;
    }
    // Show row if ANY of its flags are active
    const visible = flags.some(f => activeFilters.has(f));
    row.classList.toggle('hidden', !visible);
  });
}

function filterAP(btn, code) {
  if (activeFilters.has(code)) {
    activeFilters.delete(code);
    btn.classList.remove('active');
    btn.classList.add('inactive');
  } else {
    activeFilters.add(code);
    btn.classList.remove('inactive');
    btn.classList.add('active');
  }
  updateRows();
}

function filterAll(btn) {
  const allActive = activeFilters.size === ALL_APS.length;
  if (allActive) {
    activeFilters.clear();
    document.querySelectorAll('.filter-btn[data-ap]').forEach(b => {
      b.classList.remove('active'); b.classList.add('inactive');
    });
    btn.textContent = 'Show All';
  } else {
    activeFilters = new Set(ALL_APS);
    document.querySelectorAll('.filter-btn[data-ap]').forEach(b => {
      b.classList.add('active'); b.classList.remove('inactive');
    });
    btn.textContent = 'Show All';
  }
  updateRows();
}
</script>
</body>
</html>
"""


# ---------------------------------------------------------------------------
# Project / session helpers
# ---------------------------------------------------------------------------

def find_claude_project_dir(cwd: Path) -> Path | None:
    slug = '-' + str(cwd).replace('/', '-').lstrip('-')
    candidate = Path.home() / '.claude' / 'projects' / slug
    return candidate if candidate.exists() else None


def claude_sessions_for_date(project_dir: Path, target: date) -> list[dict]:
    results = []
    for p in sorted(project_dir.glob('*.jsonl')):
        if date.fromtimestamp(p.stat().st_mtime) == target:
            results.append({
                'source': 'claude', 'sid': p.stem[:8], 'path': p,
                'time': __import__('datetime').datetime.fromtimestamp(
                    p.stat().st_mtime).strftime('%H:%M'),
            })
    results.sort(key=lambda x: x['path'].stat().st_mtime)
    return results


def _codex_session_meta(path: Path) -> dict:
    with open(path) as f:
        for line in f:
            try:
                obj = json.loads(line)
            except json.JSONDecodeError:
                continue
            if obj.get('type') == 'session_meta':
                return obj.get('payload', {})
    return {}


def codex_sessions_for_date(cwd: Path, target: date) -> list[dict]:
    codex_home = Path(os.environ.get('CODEX_HOME', Path.home() / '.codex'))
    day_dir = codex_home / 'sessions' / f'{target:%Y}' / f'{target:%m}' / f'{target:%d}'
    results = []
    for path in sorted(day_dir.glob('rollout-*.jsonl')):
        meta = _codex_session_meta(path)
        session_cwd = meta.get('cwd')
        if not session_cwd:
            continue
        try:
            if Path(session_cwd).resolve() != cwd.resolve():
                continue
        except OSError:
            continue
        timestamp = str(meta.get('timestamp', ''))
        results.append({
            'source': 'codex',
            'sid': str(meta.get('id') or meta.get('session_id') or path.stem)[0:8],
            'path': path,
            'time': timestamp[11:16] if len(timestamp) >= 16 else '??:??',
        })
    return results


def sessions_for_date(cwd: Path, target: date, source: str) -> list[dict]:
    codex = codex_sessions_for_date(cwd, target) if source in ('auto', 'codex') else []
    claude_dir = find_claude_project_dir(cwd)
    claude = (claude_sessions_for_date(claude_dir, target)
              if claude_dir and source in ('auto', 'claude') else [])
    if source == 'auto':
        return codex + claude
    return codex if source == 'codex' else claude


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--date', default=str(date.today() - timedelta(days=1)))
    parser.add_argument('--out', default=None, help='Output file (default: auto by format)')
    parser.add_argument('--format', choices=['html', 'md'], default='html')
    parser.add_argument('--rules', default=str(DEFAULT_RULES_PATH))
    parser.add_argument('--no-via', action='store_true')
    parser.add_argument('--project', default=None, help='Project working directory (default: CWD)')
    parser.add_argument('--source', choices=['auto', 'codex', 'claude'], default='auto')
    args = parser.parse_args()

    target_date = date.fromisoformat(args.date)
    cwd = (Path(args.project) if args.project else Path.cwd()).resolve()

    rules = load_rules(Path(args.rules))
    sessions_raw = sessions_for_date(cwd, target_date, args.source)
    if not sessions_raw:
        print(f'No {args.source} sessions found for {cwd} on {target_date}', file=sys.stderr)
        sys.exit(1)

    if args.out:
        out_path = Path(args.out)
    else:
        ext = 'html' if args.format == 'html' else 'md'
        out_path = Path(f'agents/trin.docs/judge_tool_trace.{ext}')
    out_path.parent.mkdir(parents=True, exist_ok=True)

    # Build session data
    sessions_data = []
    total_calls = 0
    total_flags_list: list[str] = []

    for session in sessions_raw:
        sid, path = session['sid'], session['path']
        events_raw = parse_session(path, session['source'])
        if not events_raw:
            continue
        annotated = annotate_events(events_raw, rules, args.no_via)
        all_flags = [f for ev in annotated for f in ev['flags']]
        ap_counts = Counter(all_flags)
        total_calls += len(annotated)
        total_flags_list.extend(all_flags)
        sessions_data.append({
            'sid': sid,
            'time': f"{session['time']} · {session['source']}",
            'events': annotated,
            'flag_count': len(all_flags),
            'ap_counts': dict(ap_counts.most_common()),
        })

    flag_counts = Counter(total_flags_list)
    efficiency_flags = sum(count for code, count in flag_counts.items()
                           if code in EFFICIENCY_RULES)
    efficiency_verdict = 'YES' if efficiency_flags == 0 else 'NO'

    if args.format == 'html':
        import json as _json
        tmpl = Template(HTML_TEMPLATE)
        # tojson filter
        tmpl.globals['tojson'] = _json.dumps
        rendered = tmpl.render(
            project=cwd.name,
            target_date=target_date,
            rules=rules,
            rules_path=args.rules,
            sessions=sessions_data,
            total_calls=total_calls,
            total_flags=len(total_flags_list),
            flag_counts=flag_counts.most_common(),
            ap_codes=list(rules.keys()),
            efficiency_verdict=efficiency_verdict,
        )
        out_path.write_text(rendered)
    else:
        # Markdown fallback
        lines = [f'# Tool-Use Trace — {cwd.name} {target_date}', '']
        for s in sessions_data:
            lines.append(f'\n## Session {s["sid"]} ({s["time"]}) — {len(s["events"])} calls\n')
            for ev in s['events']:
                flags_str = '  '.join(f'`[⚠ {f}]`' for f in ev['flags'])
                lines.append(f'  `[{ev["seq"]:03d}]` **{ev["name"]}**: {ev["summary"]}' +
                              (f'\n    > {flags_str}' if flags_str else ''))
        lines += ['', '---', '## Summary', '',
                  f'**Total:** {total_calls} calls, {len(total_flags_list)} flags',
                  f'**MLflow-style tool efficiency:** {efficiency_verdict} '
                  f'({efficiency_flags} deterministic findings)', '',
                  '| AP | Count |', '|---|---|']
        for ap, cnt in flag_counts.most_common():
            lines.append(f'| `{ap}` | {cnt} |')
        out_path.write_text('\n'.join(lines))

    print(f'Written: {out_path}')
    print(f'{total_calls} tool calls · {len(total_flags_list)} flags · {len(sessions_data)} sessions')


if __name__ == '__main__':
    main()

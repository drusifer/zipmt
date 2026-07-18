import importlib.util
import json
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).parents[1] / 'agents' / 'tools' / 'trace_annotate.py'
SPEC = importlib.util.spec_from_file_location('trace_annotate', MODULE_PATH)
trace_annotate = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(trace_annotate)


class TraceAnnotateTest(unittest.TestCase):
    def test_parses_claude_tool_use(self):
        trace = self._trace({
            'type': 'tool_call',
            'raw': {'type': 'tool_use', 'name': 'Bash', 'id': 'one',
                    'input': {'command': 'make test'}},
        })
        self.assertEqual(trace_annotate.parse_tracegate_events(trace)[0]['input']['command'],
                         'make test')

    def test_parses_codex_function_call(self):
        trace = self._trace({
            'type': 'tool_call',
            'raw': {
                'type': 'function_call', 'name': 'functions.exec_command',
                'call_id': 'two', 'arguments': json.dumps({'cmd': 'make test'}),
            },
        })
        event = trace_annotate.parse_tracegate_events(trace)[0]
        self.assertEqual(event['name'], 'Bash')
        self.assertEqual(event['input']['command'], 'make test')

    def test_parses_codex_custom_tool_call(self):
        trace = self._trace({
            'type': 'tool_call',
            'raw': {
                'type': 'custom_tool_call', 'name': 'exec', 'call_id': 'three',
                'input': 'const r = await tools.exec_command({cmd:"make rust-clippy"});',
            },
        })
        event = trace_annotate.parse_tracegate_events(trace)[0]
        self.assertEqual(event['name'], 'Bash')
        self.assertEqual(event['input']['command'], 'make rust-clippy')

    def test_extracts_paths_from_codex_patch(self):
        patch = '*** Begin Patch\n*** Update File: src/main.rs\n@@\n-old\n+new\n*** End Patch'
        script = f'const patch = {json.dumps(patch)};\ntext(await tools.apply_patch(patch));'
        trace = self._trace({
            'type': 'tool_call',
            'raw': {
                'type': 'custom_tool_call', 'name': 'exec', 'call_id': 'four',
                'input': script,
            },
        })
        event = trace_annotate.parse_tracegate_events(trace)[0]
        self.assertEqual(event['name'], 'Edit')
        self.assertEqual(event['input']['file_path'], 'src/main.rs')

    def test_chat_message_tool_names_are_not_executed(self):
        command = 'make chat MSG="pytest and ruff must use make" PERSONA=Trin'
        self.assertNotIn('AP-MAKE-BYPASS', trace_annotate.classify_bash(command))

    def test_or_operator_is_not_a_make_pipeline(self):
        command = 'make rust-tools-check V=-vv; command -v perf || true'
        self.assertNotIn('AP-MAKE-PIPE', trace_annotate.classify_bash(command))

    def test_real_make_pipeline_is_flagged(self):
        self.assertIn('AP-MAKE-PIPE',
                      trace_annotate.classify_bash('make test | tail -20'))

    def test_via_rule_does_not_cross_shell_segments(self):
        command = "rg --files -g '*.toml'; python -c 'import tracegate'"
        self.assertNotIn('AP-VIA-GREP', trace_annotate.classify_bash(command))

    def test_real_symbol_grep_is_flagged(self):
        command = 'rg -n "def main|class Parser" agents/tools'
        self.assertIn('AP-VIA-GREP', trace_annotate.classify_bash(command))

    def test_real_direct_tool_is_flagged(self):
        self.assertIn('AP-MAKE-BYPASS',
                      trace_annotate.classify_bash('cd src && pytest -q'))

    def test_immediate_identical_call_is_unchanged_retry(self):
        events = [self._event('WebSearch', {'query': 'trace tools'})] * 2
        annotated = trace_annotate.annotate_events(
            events, trace_annotate.BUILTIN_RULES, no_via=False)
        self.assertIn('AP-UNCHANGED-RETRY', annotated[1]['flags'])

    def test_duplicate_tool_within_window_is_flagged(self):
        events = [
            self._event('WebSearch', {'query': 'trace tools'}),
            self._event('Bash', {'command': 'make help'}),
            self._event('WebSearch', {'query': 'trace tools'}),
        ]
        annotated = trace_annotate.annotate_events(
            events, trace_annotate.BUILTIN_RULES, no_via=False)
        self.assertIn('AP-DUP-TOOL', annotated[2]['flags'])

    def test_repeated_test_without_edit_is_flagged(self):
        events = [self._event('Bash', {'command': 'make test'})] * 2
        annotated = trace_annotate.annotate_events(
            events, trace_annotate.BUILTIN_RULES, no_via=False)
        self.assertIn('AP-RETEST-NO-CHANGE', annotated[1]['flags'])
        self.assertNotIn('AP-UNCHANGED-RETRY', annotated[1]['flags'])

    def test_edit_resets_repeated_test(self):
        events = [
            self._event('Bash', {'command': 'make test'}),
            self._event('Edit', {'file_path': 'src/main.py'}),
            self._event('Bash', {'command': 'make test'}),
        ]
        annotated = trace_annotate.annotate_events(
            events, trace_annotate.BUILTIN_RULES, no_via=False)
        self.assertNotIn('AP-RETEST-NO-CHANGE', annotated[2]['flags'])

    def test_repeated_search_without_edit_is_flagged(self):
        events = [self._event('Bash', {'command': 'rg -n TODO src'})] * 2
        annotated = trace_annotate.annotate_events(
            events, trace_annotate.BUILTIN_RULES, no_via=True)
        self.assertIn('AP-SEARCH-LOOP', annotated[1]['flags'])

    def test_stateful_polling_is_not_an_unchanged_retry(self):
        events = [self._event('write_stdin', {
            'session_id': 42, 'chars': '', 'yield_time_ms': 30000,
        })] * 2
        annotated = trace_annotate.annotate_events(
            events, trace_annotate.BUILTIN_RULES, no_via=False)
        self.assertNotIn('AP-UNCHANGED-RETRY', annotated[1]['flags'])

    @staticmethod
    def _event(name, inp):
        return {'name': name, 'input': inp, 'id': '', 'ts': ''}

    @staticmethod
    def _trace(event):
        return {'run': {'events': [event]}}


if __name__ == '__main__':
    unittest.main()

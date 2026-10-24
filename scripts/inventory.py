#!/usr/bin/env python3
"""Expand the pinned C++ runtime test cases and hash their source corpus."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

def collect(upstream):
    sha = subprocess.check_output(['git', '-C', str(upstream), 'rev-parse', 'HEAD'], text=True).strip()
    entries, unavailable = [], []
    for executable in sorted((upstream / 'build/bin').iterdir()):
        if not executable.name.startswith(('UNIT_', 'INTEGRATION_', 'REGRESSION_')):
            continue
        result = subprocess.run([str(executable), '--gtest_list_tests'], capture_output=True, text=True, timeout=30)
        if result.returncode:
            unavailable.append({'target': executable.name, 'returncode': result.returncode, 'stderr': result.stderr})
            continue
        suite = None
        for line in result.stdout.splitlines():
            if re.match(r'^\S.*\.\s*(#.*)?$', line):
                suite = line.split('#')[0].strip()
            elif line.startswith('  ') and suite:
                case = line.strip().split('#')[0].strip()
                if case:
                    entries.append({'id': f'{executable.name}:{suite}{case}', 'target': executable.name, 'case': suite + case, 'baseline': 'not_run', 'disposition': 'pending', 'replacement': [], 'review': 'pending'})
    sources = {}
    for directory in ('src', 'test', 'log', 'parameters', 'python/test'):
        for path in sorted((upstream / directory).rglob('*')):
            if path.is_file() and 'build' not in path.parts:
                sources[str(path.relative_to(upstream))] = hashlib.sha256(path.read_bytes()).hexdigest()
    symbols = {}
    pattern = re.compile(r'\bTEST(?:_F|_P)?\s*\(\s*(\w+)\s*,\s*(?:GZ_UTILS_TEST_(?:DISABLED_ON|ENABLED_ONLY_ON)_\w+\(\s*)?(\w+)')
    for filename in sources:
        if not filename.endswith(('.cc', '.hh')):
            continue
        text = (upstream / filename).read_text(errors='replace')
        for match in pattern.finditer(text):
            symbols.setdefault(f'{match[1]}.{match[2]}', []).append({'path': filename, 'line': text.count('\n', 0, match.start()) + 1, 'symbol': f'{match[1]}.{match[2]}'})
    for entry in entries:
        suite, case = entry['case'].split('.', 1)
        key = f"{suite.split('/')[-1]}.{case.split('/')[0].removeprefix('DISABLED_')}"
        entry['source'] = symbols.get(key, [])
        # Pinned CMake/header guards: these cases do not exist in a ZeroMQ-only
        # native build. Keep them in the full catalog and require them with Zenoh.
        zenoh = entry['target'].startswith('INTEGRATION_zenoh') or entry['case'].startswith('Zenoh') or entry['case'] in {'ShmHelpersTest.WithPayloadViewHeap', 'ShmHelpersTest.NoSession', 'ShmHelpersTest.DisabledInConfig'}
        entry['reference_features'] = ['zenoh'] if zenoh else []
        entry['contract'] = 'Retained original assertions; detailed per-case contract audit pending.'
        entry['configuration'] = 'Expanded on Linux x86_64; backend/environment conditions remain pending audit.'
    record = {'upstream_sha': sha, 'expansion': 'runtime --gtest_list_tests', 'entries': entries, 'unavailable_targets': unavailable, 'source_fixture_sha256': sources, 'non_gtest_scope': 'Ruby/CLI/Python/example workflows remain pending inventory; not retired.'}
    return record


if __name__ == "__main__":
    root = Path(__file__).resolve().parent.parent
    upstream = Path(os.environ.get("GZ_REFERENCE_SOURCE", str(root.parent / "gz-transport")))
    record = collect(upstream)
    previous_path = root / 'docs/upstream-tests.json'
    if previous_path.exists():
        previous = {entry['id']:entry for entry in json.loads(previous_path.read_text())['entries']}
        for entry in record['entries']:
            for field in ['baseline','disposition','replacement','review','evidence']:
                if field in previous.get(entry['id'], {}):
                    entry[field] = previous[entry['id']][field]
    (root / "docs/upstream-tests.json").write_text(json.dumps(record, indent=2) + "\n")
    print(f"{len(record['entries'])} expanded cases, {len(record['unavailable_targets'])} unavailable targets")

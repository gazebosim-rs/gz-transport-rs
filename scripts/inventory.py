#!/usr/bin/env python3
# Copyright (C) 2026 Open Source Robotics Foundation
# SPDX-License-Identifier: Apache-2.0
"""Expand the pinned C++ runtime test cases and hash their source corpus."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

def listed_cases(report):
    cases=[]
    for suite in report['testsuites']:
        if suite['tests'] != len(suite['testsuite']):
            raise ValueError('Incomplete suite enumeration')
        for case in suite['testsuite']:
            if not suite['name'] or not case['name']:
                raise ValueError('Empty runtime test name')
            cases.append(suite['name']+'.'+case['name'])
    if report['tests'] != len(cases) or len(cases) != len(set(cases)):
        raise ValueError('Incomplete or duplicate runtime enumeration')
    return cases


def collect(upstream, diagnostics=None, *, env=None):
    sha = subprocess.check_output(['git', '-C', str(upstream), 'rev-parse', 'HEAD'], text=True).strip()
    entries, unavailable = [], []
    with tempfile.TemporaryDirectory(prefix='gz-transport-enumeration-') as directory:
        output = Path(diagnostics) if diagnostics is not None else Path(directory)
        output.mkdir(parents=True, exist_ok=True)
        for executable in sorted((upstream / 'build/bin').iterdir()):
            if not executable.name.startswith(('UNIT_', 'INTEGRATION_', 'REGRESSION_')):
                continue
            listing=output / f'{executable.name}.json'
            listing.unlink(missing_ok=True)
            result = subprocess.run([str(executable), '--gtest_list_tests', '--gtest_filter=*', '--gtest_color=no', f'--gtest_output=json:{listing}'], capture_output=True, text=True, timeout=30, env=env)
            if diagnostics is not None:
                (output/f'{executable.name}.stdout').write_text(result.stdout)
                (output/f'{executable.name}.stderr').write_text(result.stderr)
            try:
                if result.returncode:
                    raise ValueError(f'Native enumeration exit {result.returncode}')
                cases=listed_cases(json.loads(listing.read_text()))
            except (ValueError,KeyError,OSError,TypeError) as error:
                unavailable.append({'target':executable.name,'error':str(error),'stderr':result.stderr})
                continue
            for case in cases:
                entries.append({'id': f'{executable.name}:{case}', 'target': executable.name, 'case':case, 'baseline': 'not_run', 'disposition': 'pending', 'replacement': [], 'review': 'pending'})
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
    record = {'upstream_sha': sha, 'expansion': 'runtime --gtest_list_tests --gtest_output=json', 'entries': entries, 'unavailable_targets': unavailable, 'source_fixture_sha256': sources, 'non_gtest_scope': 'Ruby/CLI/Python/example workflows remain pending inventory; not retired.'}
    return record


if __name__ == "__main__":
    root = Path(__file__).resolve().parent.parent
    upstream = Path(os.environ.get("GZ_REFERENCE_SOURCE", str(root.parent / "gz-transport")))
    native_env = dict(os.environ)
    if prefix := native_env.get('GZ_REFERENCE_PREFIX'):
        native_env['LD_LIBRARY_PATH'] = str(Path(prefix) / 'lib') + ':' + native_env.get('LD_LIBRARY_PATH', '')
    record = collect(upstream, env=native_env)
    previous_path = root / 'docs/upstream-tests.json'
    if previous_path.exists():
        previous = {entry['id']:entry for entry in json.loads(previous_path.read_text())['entries']}
        for entry in record['entries']:
            for field in ['baseline','disposition','replacement','review','evidence']:
                if field in previous.get(entry['id'], {}):
                    entry[field] = previous[entry['id']][field]
    (root / "docs/upstream-tests.json").write_text(json.dumps(record, indent=2) + "\n")
    print(f"{len(record['entries'])} expanded cases, {len(record['unavailable_targets'])} unavailable targets")

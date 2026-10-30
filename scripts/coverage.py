#!/usr/bin/env python3
# Copyright (C) 2026 Open Source Robotics Foundation
# SPDX-License-Identifier: Apache-2.0
"""Strict LLVM line/region accounting; fail on missing/empty instrumentation."""
import argparse
import json
from pathlib import Path


def above_95(covered, count):
    return count > 0 and 0 <= covered <= count and covered * 100 > count * 95


def owned_crates(manifest):
    groups = manifest.get('owned_crates', {manifest['crate']: manifest['production_files']})
    if not isinstance(groups, dict) or not groups or any(not isinstance(files, list) or not files for files in groups.values()):
        raise ValueError('Missing owned crate source inventories')
    flattened = [name for files in groups.values() for name in files]
    if len(flattened) != len(set(flattened)) or set(flattened) != set(manifest['production_files']):
        raise ValueError('Owned crate inventories must partition all production files')
    return groups


def check(report, manifest, root):
    if report.get('type') != 'llvm.coverage.json.export':
        raise ValueError('Missing LLVM coverage export')
    expected = {str((root / name).resolve()) for name in manifest['production_files']}
    actual = {str(p.resolve()) for directory in manifest['production_roots'] for p in (root / directory).rglob('*.rs')}
    if expected != actual or len(expected) != len(manifest['production_files']) or manifest['exclusions']:
        raise ValueError('Production inventory mismatch or unreviewed exclusion')
    files = {}
    for data in report['data']:
        for file in data['files']:
            path = Path(file['filename']).resolve()
            filename = str(path)
            if filename in expected:
                if filename in files:
                    raise ValueError('Duplicate owned LLVM source')
                files[filename] = file
            elif path.is_relative_to(root) and not (path.suffix == '.rs' and any(path.is_relative_to(root / directory) for directory in manifest.get('test_roots', ['tests']))):
                raise ValueError(f'Unaccounted owned LLVM input: {path}')
    totals = {metric: {'covered': 0, 'count': 0} for metric in ('lines', 'regions')}
    for filename in expected:
        if filename not in files:
            raise ValueError(f'Missing production instrumentation: {filename}')
        for metric in totals:
            values = files[filename]['summary'][metric]
            if values['count'] <= 0:
                raise ValueError(f'Empty production denominator: {filename}/{metric}')
            for key in ('covered', 'count'):
                totals[metric][key] += values[key]
    for metric, values in totals.items():
        if not above_95(**values):
            raise ValueError(f'{manifest["crate"]}/{manifest["profile"]}/{metric}: {values} must be strictly >95%')
    groups = owned_crates(manifest)
    if 'owned_crates' in manifest:
        per_crate = {}
        for crate, names in groups.items():
            per_crate[crate] = {}
            for metric in ('lines', 'regions'):
                values = {key: sum(files[str((root / name).resolve())]['summary'][metric][key] for name in names) for key in ('covered', 'count')}
                if not above_95(**values):
                    raise ValueError(f'{crate}/{manifest["profile"]}/{metric}: {values} must be strictly >95%')
                per_crate[crate][metric] = values
        totals['crates'] = per_crate
    return totals


def changed_check(lcov, changed, root, production_files=None):
    lines, filename = {}, None
    for line in lcov.split("\n"):
        if line.startswith('SF:'):
            filename = str(Path(line[3:]).resolve())
            if filename in lines:
                raise ValueError('Duplicate LCOV source')
            lines[filename] = {}
        elif line.startswith('DA:'):
            number, count, *_ = line[3:].split(',')
            number, count = int(number), int(count)
            if filename is None or number <= 0 or count < 0 or number in lines[filename]:
                raise ValueError('Invalid or duplicate LCOV line')
            lines[filename][number] = count
    # check() already requires a nonempty executable JSON denominator per file.
    for name in production_files if production_files is not None else changed:
        if not lines.get(str((root / name).resolve())):
            raise ValueError('Missing executable LCOV production source')
    counts = [lines[str((root / name).resolve())][number] for name, numbers in changed.items() for number in numbers if number in lines.get(str((root / name).resolve()), {})]
    if counts and not above_95(sum(n > 0 for n in counts), len(counts)):
        raise ValueError('Changed executable production lines must be strictly >95%')
    return {'covered': sum(n > 0 for n in counts), 'count': len(counts)}


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('report', type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    print(json.dumps(check(json.loads(args.report.read_text()), json.loads((root / 'coverage.json').read_text()), root)))

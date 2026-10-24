#!/usr/bin/env python3
# Copyright (C) 2026 Open Source Robotics Foundation
# SPDX-License-Identifier: Apache-2.0
"""One entry point for CI and exact staged-tree local verification."""
import argparse
import glob
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
from coverage import check, changed_check
from inventory import collect
from reference import check_ledger, check_mapped_tests, enabled_features

ROOT = Path(__file__).resolve().parent.parent


def run(args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def validate_evidence(evidence, root, profile):
    if evidence['profile'] != profile:
        raise ValueError('Stale coverage profile')
    for filename, digest in evidence['inputs'].items():
        if hashlib.sha256((root / filename).read_bytes()).hexdigest() != digest:
            raise ValueError(f'Stale evidence/source changed during verification: {filename}')


def changed_lines(diff):
    changed, filename = {}, None
    for line in diff.splitlines():
        if line.startswith('+++ b/'):
            filename = line[6:]
        elif line.startswith('@@') and filename:
            start, _, count = re.search(r'\+(\d+)(,(\d+))?', line).groups()
            changed.setdefault(filename, set()).update(range(int(start), int(start) + int(count or 1)))
    return changed


def source_changes(root, production_files, base=None):
    if base and not re.fullmatch(r'[0-9a-f]{40}', base):
        raise ValueError('Invalid coverage base SHA')
    if base == '0' * 40:
        return {p: set(range(1, len((root / p).read_text().splitlines()) + 1)) for p in production_files}
    if base:
        run(['git', 'cat-file', '-e', f'{base}^{{commit}}'], cwd=root, capture_output=True)
    diff = subprocess.check_output(['git', 'diff', '--no-ext-diff', '--unified=0', base or 'HEAD', '--', 'src'], cwd=root, text=True)
    if not base and not diff and subprocess.run(['git', 'rev-parse', '--verify', 'HEAD^'], cwd=root, capture_output=True).returncode == 0:
        diff = subprocess.check_output(['git', 'diff', '--no-ext-diff', '--unified=0', 'HEAD^', 'HEAD', '--', 'src'], cwd=root, text=True)
    changed = changed_lines(diff)
    tracked = set(subprocess.check_output(['git', 'ls-files', '--', 'src'], cwd=root, text=True).splitlines())
    for filename in set(production_files) - tracked:
        changed[filename] = set(range(1, len((root / filename).read_text().splitlines()) + 1))
    return changed


def staged():
    with tempfile.TemporaryDirectory(prefix='gz-transport-staged-') as directory:
        candidate = Path(directory) / ROOT.name
        candidate.mkdir()
        run(['git', 'checkout-index', '--all', f'--prefix={candidate}/'], cwd=ROOT)
        env = dict(os.environ, CARGO_TARGET_DIR=str(ROOT / 'target/staged'), GZ_REFERENCE_SOURCE=os.environ.get('GZ_REFERENCE_SOURCE', str(ROOT.parent / 'gz-transport')))
        has_head = subprocess.run(['git', 'rev-parse', '--verify', 'HEAD'], cwd=ROOT, capture_output=True).returncode == 0
        diff = subprocess.check_output(['git', 'diff', '--cached', '--no-ext-diff', '--unified=0', *(['HEAD'] if has_head else []), '--', 'src'], cwd=ROOT, text=True)
        changed_file = Path(directory) / 'changed-lines.json'
        changed_file.write_text(json.dumps({name: sorted(lines) for name, lines in changed_lines(diff).items()}))
        env['GZ_CHANGED_LINES_FILE'] = str(changed_file)
        run([sys.executable, 'scripts/verify.py', 'verify', '--profile', 'zeromq-linux'], cwd=candidate, env=env)
        destination = ROOT / 'artifacts/staged'
        shutil.rmtree(destination, ignore_errors=True)
        shutil.copytree(candidate / 'artifacts', destination)


def verify(profile):
    manifest = json.loads((ROOT / 'coverage.json').read_text())
    if profile != manifest['profile']:
        raise ValueError(f'Missing required coverage profile: {profile}')
    inputs_paths = [*manifest['production_files'], 'Cargo.lock', 'Cargo.toml', 'rust-toolchain.toml', 'coverage.json', 'docs/upstream-tests.json', 'docs/reference-catalog.json', 'docs/reference-lock.json']
    inputs_paths += [str(p.relative_to(ROOT)) for directory in ['tests', 'scripts'] for p in (ROOT / directory).rglob('*') if p.is_file() and '__pycache__' not in p.parts]
    inputs = {p: hashlib.sha256((ROOT / p).read_bytes()).hexdigest() for p in inputs_paths}
    os.chdir(ROOT)
    env = dict(os.environ, RUSTUP_TOOLCHAIN=manifest['toolchain'])
    env.pop('CARGO_ENCODED_RUSTFLAGS', None)
    prefix = Path(env.get('GZ_REFERENCE_PREFIX', '/home/nkoenig/local')).resolve()
    env['LD_LIBRARY_PATH'] = str(prefix / 'lib') + ':' + env.get('LD_LIBRARY_PATH', '')
    run(['cargo', 'fmt', '--check'], env=env)
    run(['cargo', 'clippy', '--locked', '--all-targets', '--', '-D', 'warnings'], env=env)
    run([sys.executable, '-m', 'unittest', 'discover', '-s', 'scripts', '-p', 'test_*.py'])
    run(['bash', 'scripts/build-cpp-peer.sh'], env=env)
    linkage = subprocess.check_output(['ldd', str(ROOT / 'artifacts/cpp-peer')], env=env, text=True)
    (ROOT / 'artifacts/cpp-linkage.txt').write_text(linkage)
    libraries = re.findall(r'libgz-[^ ]+\s+=>\s+(\S+)', linkage)
    if len(libraries) < 2 or any(not Path(path).resolve().is_relative_to(prefix) for path in libraries):
        raise ValueError('C++ peer linked outside the selected reference prefix')
    # Required original assertions are executed, not inferred from source names.
    reference = Path(os.environ.get('GZ_REFERENCE_SOURCE', str(ROOT.parent / 'gz-transport')))
    baseline = json.loads((ROOT / 'docs/reference.json').read_text())
    sha = subprocess.check_output(['git', '-C', str(reference), 'rev-parse', 'HEAD'], text=True).strip()
    if sha != baseline['upstream_sha']:
        raise ValueError('C++ reference SHA mismatch')
    ledger = json.loads((ROOT / 'docs/upstream-tests.json').read_text())
    expanded = collect(reference)
    catalog = json.loads((ROOT / 'docs/reference-catalog.json').read_text())
    reference_counts = check_ledger(ledger, expanded, enabled_features(reference), catalog)
    run(['git', '-C', str(reference), 'diff', '--exit-code', 'HEAD'])
    run([str(reference / 'build/bin/UNIT_TopicUtils_TEST'), '--gtest_filter=TopicUtilsTest.testFullyQualifiedName'], env=env)
    artifacts = ROOT / 'artifacts'
    profiles = artifacts / 'profraw'
    shutil.rmtree(profiles, ignore_errors=True)
    profiles.mkdir(parents=True)
    target = Path(env.get('CARGO_TARGET_DIR', str(ROOT / 'target'))) / 'coverage'
    wrapper_source = ROOT / 'scripts/rustc-coverage.py'
    wrapper = artifacts / f'rustc-coverage-{hashlib.sha256(wrapper_source.read_bytes()).hexdigest()}.py'
    shutil.copy2(wrapper_source, wrapper)
    # Cargo includes the workspace wrapper path in its artifact hashes. The
    # content digest prevents reuse after an instrumentation wrapper change.
    env.update(RUSTFLAGS='', RUSTC_WORKSPACE_WRAPPER=str(wrapper), CARGO_TARGET_DIR=str(target), LLVM_PROFILE_FILE=str(profiles / '%m-%p.profraw'))
    with (artifacts / 'test-build.jsonl').open('w') as output:
        run(['cargo', 'test', '--locked', '--message-format=json', '--', '--test-threads=1'], stdout=output, env=env)
    check_mapped_tests(ledger, (artifacts / 'test-build.jsonl').read_text())
    sysroot = Path(subprocess.check_output(['rustc', '--print', 'sysroot'], env=env, text=True).strip())
    host = re.search(r'^host: (.+)$', subprocess.check_output(['rustc', '-vV'], env=env, text=True), re.M)[1]
    llvm = sysroot / 'lib/rustlib' / host / 'bin'
    raw = glob.glob(str(profiles / '*.profraw'))
    if not raw:
        raise ValueError('Missing LLVM instrumentation')
    run([str(llvm / 'llvm-profdata'), 'merge', '-sparse', *raw, '-o', str(artifacts / 'coverage.profdata')])
    objects = []
    for line in (artifacts / 'test-build.jsonl').read_text().splitlines():
        try:
            record = json.loads(line)
        except json.JSONDecodeError:
            continue
        if record.get('reason') == 'compiler-artifact' and record.get('executable') and record.get('profile', {}).get('test'):
            objects.extend(['-object', record['executable']])
    if not objects:
        raise ValueError('Missing test executables')
    command = [str(llvm / 'llvm-cov'), 'export', '--fatal-warnings', '-num-threads=1', f'-instr-profile={artifacts}/coverage.profdata', *objects]
    with (artifacts / 'coverage.json').open('w') as output:
        run(command, stdout=output)
    lcov = subprocess.check_output([*command, '-format=lcov'], text=True)
    (artifacts / 'coverage.lcov').write_text(lcov)
    report = json.loads((artifacts / 'coverage.json').read_text())
    totals = check(report, manifest, ROOT)
    if 'GZ_CHANGED_LINES_FILE' in env:
        changed = json.loads(Path(env['GZ_CHANGED_LINES_FILE']).read_text())
    elif (ROOT / '.git').exists():
        changed = source_changes(ROOT, manifest['production_files'], env.get('GZ_COVERAGE_BASE'))
    else:
        changed = {p: set(range(1, len((ROOT / p).read_text().splitlines()) + 1)) for p in manifest['production_files']}
    changed_totals = changed_check(lcov, changed, ROOT, manifest['production_files'])
    evidence = {'profile': profile, 'toolchain': manifest['toolchain'], 'inputs': inputs, 'totals': totals, 'changed_lines': changed_totals, 'upstream_sha': sha, 'reference': reference_counts, 'cpp_libraries_sha256': {Path(path).name:hashlib.sha256(Path(path).read_bytes()).hexdigest() for path in libraries}, 'runner': subprocess.check_output(['uname', '-a'], text=True).strip()}
    validate_evidence(evidence, ROOT, profile)
    (artifacts / 'evidence.json').write_text(json.dumps(evidence, indent=2) + '\n')
    print(json.dumps({'profile':profile,'totals':totals,'changed_lines':changed_totals,'evidence':str(artifacts/'evidence.json')},indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('command', choices=['verify'])
    parser.add_argument('--staged', action='store_true')
    parser.add_argument('--profile', default='zeromq-linux')
    parser.add_argument('--release-matrix', action='store_true')
    args = parser.parse_args()
    if args.release_matrix:
        ledger = json.loads((ROOT / 'docs/upstream-tests.json').read_text())
        if any(entry['disposition'] == 'pending' or entry['review'] == 'pending' for entry in ledger['entries']) or 'pending' in ledger['non_gtest_scope']:
            raise ValueError('Release qualification blocked by retained-scope pending tests/review')
    if args.staged:
        staged()
    else:
        verify(args.profile)

#!/usr/bin/env python3
# Copyright (C) 2026 Open Source Robotics Foundation
# SPDX-License-Identifier: Apache-2.0
"""One entry point for CI and exact staged-tree local verification."""
import argparse
import glob
import hashlib
import json
import os
import platform
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


def staged_head(root):
    result = subprocess.run(['git', 'rev-parse', '--verify', 'HEAD'], cwd=root, text=True, capture_output=True)
    return result.stdout.strip() if result.returncode == 0 else None


def staged_tree(root):
    return subprocess.check_output(['git', 'write-tree'], cwd=root, text=True).strip()


def validate_snapshot(root, tree, candidate):
    algorithm = subprocess.check_output(['git', 'rev-parse', '--show-object-format'], cwd=root, text=True).strip()
    expected = set()
    for entry in subprocess.check_output(['git', 'ls-tree', '-rz', tree], cwd=root).split(b'\0'):
        if not entry:
            continue
        metadata, name = entry.split(b'\t', 1)
        mode, kind, oid = metadata.split()
        relative = os.fsdecode(name)
        expected.add(relative)
        path = candidate / relative
        if kind != b'blob':
            raise ValueError('Unsupported non-blob staged input')
        if mode == b'120000' and path.is_symlink():
            data = os.fsencode(os.readlink(path))
        elif mode in (b'100644', b'100755') and path.is_file() and not path.is_symlink():
            if bool(path.stat().st_mode & 0o100) != (mode == b'100755'):
                raise ValueError('Exported staged executable mode differs from Git tree')
            data = path.read_bytes()
        else:
            raise ValueError('Exported staged file type differs from Git tree')
        digest = hashlib.new(algorithm, f'blob {len(data)}\0'.encode() + data).hexdigest()
        if digest != oid.decode():
            raise ValueError(f'Exported staged blob differs from Git tree: {relative}')
    actual = {str(path.relative_to(candidate)) for path in candidate.rglob('*') if path.is_file() or path.is_symlink()}
    if actual != expected:
        raise ValueError('Exported staged paths differ from Git tree')


def staged():
    tree, head = staged_tree(ROOT), staged_head(ROOT)
    with tempfile.TemporaryDirectory(prefix='gz-transport-staged-') as directory:
        candidate = Path(directory) / ROOT.name
        candidate.mkdir()
        # A private index preserves every blob, including export-ignore paths.
        snapshot_env = dict(os.environ, GIT_INDEX_FILE=str(Path(directory) / 'index'))
        run(['git', 'read-tree', tree], cwd=ROOT, env=snapshot_env)
        run(['git', 'checkout-index', '--all', f'--prefix={candidate}/'], cwd=ROOT, env=snapshot_env)
        validate_snapshot(ROOT, tree, candidate)
        env = dict(os.environ, CARGO_TARGET_DIR=str(ROOT / 'target/staged'), GZ_REFERENCE_SOURCE=os.environ.get('GZ_REFERENCE_SOURCE', str(ROOT.parent / 'gz-transport')))
        base = head or subprocess.check_output(['git', 'mktree'], cwd=ROOT, input='', text=True).strip()
        diff = subprocess.check_output(['git', 'diff', '--no-ext-diff', '--unified=0', base, tree, '--', 'src'], cwd=ROOT, text=True)
        changed_file = Path(directory) / 'changed-lines.json'
        changed_file.write_text(json.dumps({name: sorted(lines) for name, lines in changed_lines(diff).items()}))
        env['GZ_CHANGED_LINES_FILE'] = str(changed_file)
        run([sys.executable, 'scripts/verify.py', 'verify', '--profile', 'zeromq-linux'], cwd=candidate, env=env)
        if staged_tree(ROOT) != tree or staged_head(ROOT) != head:
            raise ValueError('Git index or HEAD changed during staged verification')
        destination = ROOT / 'artifacts/staged'
        shutil.rmtree(destination, ignore_errors=True)
        shutil.copytree(candidate / 'artifacts', destination)


def check_layout(root, metadata, manifest):
    if manifest['crate'] != 'gz-transport' or manifest['production_roots'] != ['src'] or manifest['exclusions']:
        raise ValueError('Unreviewed production layout or exclusions')
    if manifest['required_runners'] != ['linux-x86_64-multicast', 'pinned-cpp-peer'] or (platform.system(), platform.machine()) != ('Linux', 'x86_64'):
        raise ValueError('Required native runner unavailable or matrix changed')
    owned = [package for package in metadata['packages'] if package['source'] is None]
    if len(owned) != 1:
        raise ValueError('Additional local packages require production coverage support')
    package = owned[0]
    if package['name'] != 'gz-transport' or Path(package['manifest_path']).resolve() != root / 'Cargo.toml' or metadata['workspace_members'] != [package['id']]:
        raise ValueError('Unreviewed Cargo package or workspace')
    if package['features']:
        raise ValueError('Feature profiles require explicit coverage support')
    if package['license'] != 'Apache-2.0':
        raise ValueError('Owned package must remain Apache-2.0')
    libraries = 0
    for target in package['targets']:
        path = Path(target['src_path']).resolve()
        if target['kind'] == ['lib'] and target['crate_types'] == ['lib'] and path == root / 'src/lib.rs':
            libraries += 1
        elif target['kind'] != ['test'] or not path.is_relative_to(root / 'tests'):
            raise ValueError('Unaccounted Cargo target requires production coverage support')
    if libraries != 1:
        raise ValueError('Exactly one production library is required')
    for directory, children, files in os.walk(root):
        if Path(directory) == root:
            children[:] = [name for name in children if name not in ('.git', 'target', 'artifacts')]
        for name in files:
            path = (Path(directory) / name).resolve()
            if name.endswith('.rs') and not (path.is_relative_to(root / 'src') or path.is_relative_to(root / 'tests')):
                raise ValueError(f'Unaccounted owned Rust source: {path}')
    return package


def depinfo_inputs(text, root, production_files):
    expected = {(root / name).resolve() for name in production_files}
    sources, rules = [], []
    def paths(parts):
        # This profile has plain Rust paths. Refuse ambiguous Make escaping,
        # rather than guessing which compiler inputs a transformed name denotes.
        if any(not part or any(c.isspace() or c in "\\$#" for c in part) for part in parts):
            raise ValueError('Unsupported compiler dep-info path encoding')
        return [(root / part).resolve() for part in parts]
    for line in text.splitlines():
        if not line or line.startswith('# env-dep:'):
            continue
        if line.endswith(':'):
            sources.extend(paths([line[:-1]]))
        elif ': ' in line:
            rules.append(paths([part for part in line.split(': ', 1)[1].split(' ') if part]))
        else:
            raise ValueError('Unrecognized compiler dep-info record')
    if not rules or len(sources) != len(expected) or set(sources) != expected or any(len(rule) != len(expected) or set(rule) != expected for rule in rules):
        raise ValueError('Normal library compiler inputs differ from production inventory')
    return sorted(str(path.relative_to(root)) for path in expected)


def production_inputs(root, production_files, package_id, env, artifact_dir):
    artifact_dir.mkdir(parents=True, exist_ok=True)
    proof = artifact_dir / 'production-inputs.json'
    proof.unlink(missing_ok=True)
    probe_env = dict(env, RUSTFLAGS='')
    probe_env.pop('CARGO_ENCODED_RUSTFLAGS', None)
    probe_env.pop('RUSTC_WORKSPACE_WRAPPER', None)
    # Cargo's timestamp cache can miss a changed module graph after restored
    # mtimes. Clean only this package's normal dev outputs, keeping dependencies.
    run(['cargo', 'clean', '--package', 'gz-transport', '--profile', 'dev'], cwd=root, env=probe_env)
    output = subprocess.check_output(['cargo', 'rustc', '--locked', '--lib', '--message-format=json'], cwd=root, env=probe_env, text=True)
    artifacts = [json.loads(line) for line in output.splitlines()]
    libraries = [record for record in artifacts if record.get('reason') == 'compiler-artifact' and record['package_id'] == package_id and record['target']['kind'] == ['lib'] and not record['profile']['test']]
    if len(libraries) != 1 or libraries[0]['fresh']:
        raise ValueError('Missing unique normal library compiler artifact')
    outputs = [Path(path) for path in libraries[0]['filenames'] if path.endswith('.rmeta')]
    if len(outputs) != 1:
        raise ValueError('Missing normal library dep-info artifact')
    depinfo = outputs[0].with_name(outputs[0].stem.removeprefix('lib') + '.d')
    raw = depinfo.read_bytes()
    inputs = depinfo_inputs(raw.decode(), root, production_files)
    evidence = {'inputs': inputs, 'depinfo_sha256': hashlib.sha256(raw).hexdigest()}
    # Do not publish raw env-dep values from compiler dep-info.
    proof.write_text(json.dumps(evidence, indent=2) + '\n')
    return evidence


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
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--format-version=1'], cwd=ROOT, env=env, text=True))
    package = check_layout(ROOT, metadata, manifest)
    run(['cargo', 'fmt', '--check'], env=env)
    library_inputs = production_inputs(ROOT, manifest['production_files'], package['id'], env, ROOT / 'artifacts')
    run(['cargo', 'clippy', '--locked', '--all-targets', '--', '-D', 'warnings'], env=env)
    run([sys.executable, '-m', 'unittest', 'discover', '-s', 'scripts', '-p', 'test_*.py'], env=env)
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
    expanded = collect(reference, ROOT / 'artifacts/reference-enumeration', env=env)
    catalog = json.loads((ROOT / 'docs/reference-catalog.json').read_text())
    reference_counts = check_ledger(ledger, expanded, enabled_features(reference), catalog)
    run(['git', '-C', str(reference), 'diff', '--exit-code', 'HEAD'])
    run([str(reference / 'build/bin/UNIT_TopicUtils_TEST'), '--gtest_filter=TopicUtilsTest.testFullyQualifiedName:TopicUtilsTest.asValidTopic'], env=env)
    run([str(reference / 'build/bin/UNIT_TopicStorage_TEST'), '--gtest_filter=TopicStorageTest.TopicStorageAPI'], env=env)
    run([str(reference / 'build/bin/UNIT_Node_TEST'), '--gtest_filter=NodeTest.ServiceWithoutOutputCallAsync:NodeTest.AdvertiseTwoEqualTopics:NodeTest.AdvertiseSameTopicSameTypeOnSameNode:NodeTest.AdvertiseRelativeTopicTwice:NodeTest.AdvertiseNamespacedTopicTwice:NodeTest.ServiceCallSync:NodeTest.ServiceCallWithoutInputSync:NodeTest.ServiceCallSyncTimeout:NodeTest.ServiceCallWithoutInputSyncTimeout:NodeTest.MultipleServiceCallAsync:NodeTest.MultipleServiceCallWithoutInputAsync:NodeTest.MultipleServiceWithoutOutputCallAsync'], env=env)
    run([str(reference / 'build/bin/UNIT_NodeOptions_TEST'), '--gtest_filter=NodeOptionsTest.accessors:NodeOptionsTest.ignPartition'], env=env)
    run([str(reference / 'build/bin/INTEGRATION_twoProcsPubSub'), '--gtest_filter=twoProcPubSub.TopicListSubscriberOnly:twoProcPubSub.TopicListRemovesUnsubscribed'], env=env)
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
    evidence = {'profile': profile, 'toolchain': manifest['toolchain'], 'inputs': inputs, 'totals': totals, 'changed_lines': changed_totals, 'upstream_sha': sha, 'reference': reference_counts, 'production_inputs': library_inputs, 'cpp_libraries_sha256': {Path(path).name:hashlib.sha256(Path(path).read_bytes()).hexdigest() for path in libraries}, 'runner': subprocess.check_output(['uname', '-a'], text=True).strip()}
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

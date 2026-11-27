#!/usr/bin/env python3
"""Check visible structural completeness through Code Mode stdio and CLI (no daemon)."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile

from bench import Client, environment, require

QUERY = 'len(search_text(query="notice_target()", structural=True, language="rust", limit=1, max_per_file=1))'


def envelope(text):
    value, sep, extra = text.partition('\n\nHost-call completeness: ')
    value = json.loads(value)
    if sep:
        return {'value': value, **json.loads(extra)}
    return value if isinstance(value, dict) else {'value': value}


CASES = {
    'ordinary-exact': ([('a', 2), ('b', 1)], 0, 2),
    'dense-lower-bound': ([('a', 2), ('b', 1), ('dense', 1025)], 1, 2),
    'early-exit-lower-bound': ([(f'f{i:02}', 1) for i in range(20)], 4, 15),
}


def check(text, unparsed, omitted):
    inexact = unparsed > 0
    result = envelope(text)
    require(result.get('value') == 1, 'expected one visible structural hit')
    notices = result.get('notices', [])
    require(len(notices) == 1, 'expected exactly one visible completeness notice')
    n = notices[0]
    require(n.get('op') == 'search_text' and n.get('call') == 1, 'unexpected call attribution')
    require(n.get('withheld_files') == 0, 'synthetic Rust files should be admitted')
    require(n.get('unparsed_files') == unparsed, f'unexpected unparsed count: {n}')
    require(n.get('omitted_rows') == omitted, f'unexpected omission count: {n}')
    require(n.get('omitted_rows_inexact', False) is inexact, f'wrong exact/lower-bound flag: {n}')
    require(n.get('incompatible_grammar_files', 0) == 0, 'all fixtures use Rust')
    require('dense.rs' not in text, 'notice disclosed skipped path')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    binary = parser.parse_args().binary.resolve(strict=True)
    failures = []
    with tempfile.TemporaryDirectory(prefix='sf-notice-') as tmp:
        for case, (files, unparsed, omitted) in CASES.items():
            root = Path(tmp) / case
            root.mkdir()
            for name, hits in files:
                (root / f'{name}.rs').write_text(f'fn {name}() {{\n' + '    notice_target();\n' * hits + '}\n', encoding='utf8')
            for surface in ['cli', 'stdio']:
                home = Path(tmp) / f'home-{case}-{surface}'
                home.mkdir()
                env = environment(home)
                try:
                    if surface == 'stdio':
                        with Client(binary, root, env) as client:
                            client.initialize()
                            reply = client.code(QUERY)
                            require(not reply.result.get('isError'), 'code_execute returned an error')
                            text = reply.text
                    else:
                        text = subprocess.run([str(binary), 'code', 'run', '-c', QUERY, '--root', str(root)], cwd=root, env=env, check=True, capture_output=True, text=True, timeout=45).stdout
                    check(text, unparsed, omitted)
                except (RuntimeError, ValueError, subprocess.SubprocessError) as exc:
                    failures.append(f'{surface}/{case}: {exc}')
                else:
                    print(f'PASS {surface}/{case}', flush=True)
    for item in failures:
        print(f'FAIL {item}')
    return int(bool(failures))


if __name__ == '__main__':
    raise SystemExit(main())

#!/usr/bin/env python3
"""Measures Zoekt against ripgrep, both inside WSL, on the frozen held out queries.

Zoekt does not build on Windows, so it cannot be timed beside the code index directly. Instead it is
timed beside ripgrep 14.1.1 (the version Claude Code embeds) on a copy of the corpora in WSL's own
filesystem, and its speed is reported as a ratio to that ripgrep, which is comparable with the
index's ratio to ripgrep on Windows. Zoekt is asked through zoekt-webserver's JSON API, the way it is
deployed, with its shards already loaded.

    python3 compare-zoekt.py <queries folder> <out file> [--probe] [--corpora a,b] [--reps 3]
"""
import json
import os
import re
import shutil
import statistics
import subprocess
import sys
import time
import urllib.request

E = os.path.expanduser('~/zoekt-eval')
CORPORA = ['unluminous@3eddcb6c3039', 'inillucent@e6aa248e3f88', 'ai-service@d8df63eb5d4e', 'linux@v6.16']
VCS = ['.git', '.svn', '.hg', '.bzr', '.jj', '.sl']
PORT = 6071


def grep_pattern(q):
    """The regex an agent hands the Grep tool for a query, as tools/search-eval/lib/rg.mjs builds it."""
    pattern = re.sub(r'[\\^$.|?*+()\[\]{}]', lambda m: '\\' + m.group(0), q['pattern']) if q.get('fixed') else q['pattern']
    return f'\\b(?:{pattern})\\b' if q.get('word') else pattern


def rg_args(q):
    """Claude Code's Grep arguments, without --json: path and line number are all that is compared."""
    args = [f'{E}/bin/rg', '--no-config', '--hidden']
    for d in VCS:
        args += ['--glob', f'!{d}']
    if q.get('ignoreCase'):
        args.append('-i')
    args += ['-n', '--with-filename', '--no-heading', '--null']
    pattern = grep_pattern(q)
    args += ['-e', pattern]
    for g in q.get('globs') or []:
        args += ['--glob', g]
    args.append(q.get('path') or '.')
    return args


def rg_lines(stdout):
    """`path:line` pairs from `path NUL line:` records."""
    out = set()
    for rec in stdout.split('\n'):
        if '\0' in rec:
            p, rest = rec.split('\0', 1)
            n = rest.split(':', 1)[0]
            if n.isdigit():
                out.add(f"{p.removeprefix('./')}:{n}")
    return out


def glob_regex(glob):
    """A ripgrep glob as an RE2 regex on a path: no slash means the file name at any depth."""
    body = glob if '/' in glob else f'**/{glob}'
    out, i = '', 0
    while i < len(body):
        c = body[i]
        if body.startswith('**/', i):
            out += '(.*/)?'
            i += 3
            continue
        if c == '*':
            out += '[^/]*'
        elif c == '?':
            out += '[^/]'
        elif c == '{':
            j = body.index('}', i)
            out += '(' + '|'.join(re.escape(x) for x in body[i + 1:j].split(',')) + ')'
            i = j
        else:
            out += re.escape(c)
        i += 1
    return f'^{out}$'


def quote(text, doubled):
    """Text inside a double quoted Zoekt term."""
    if doubled:
        text = text.replace('\\', '\\\\')
    return '"' + text.replace('"', '\\"') + '"'


def zoekt_query(q, doubled=True):
    """The Zoekt query that means what the ripgrep call means."""
    parts = ['case:no' if q.get('ignoreCase') else 'case:yes', quote(grep_pattern(q), doubled)]
    path = (q.get('path') or '').strip('/')
    if path and path != '.':
        parts.append(f'file:{quote("^" + re.escape(path) + "(/|$)", doubled)}')
    pos = [g for g in q.get('globs') or [] if not g.startswith('!')]
    neg = [g[1:] for g in q.get('globs') or [] if g.startswith('!')]
    if pos:
        parts.append('(' + ' or '.join(f'file:{quote(glob_regex(g), doubled)}' for g in pos) + ')')
    for g in neg:
        parts.append(f'-file:{quote(glob_regex(g), doubled)}')
    return ' '.join(parts)


def zoekt_search(query):
    """Asks the web server; answers the `path:line` set and the time the request took."""
    body = json.dumps({'Q': query, 'Opts': {'ShardMaxMatchCount': 10**7, 'TotalMaxMatchCount': 10**7, 'MaxDocDisplayCount': 10**6, 'NumContextLines': 0}}).encode()
    req = urllib.request.Request(f'http://127.0.0.1:{PORT}/api/search', data=body, headers={'Content-Type': 'application/json'})
    started = time.perf_counter()
    try:
        with urllib.request.urlopen(req, timeout=120) as resp:
            data = json.load(resp)
    except urllib.error.HTTPError as e:
        raise ValueError(f'zoekt refused {query!r}: {e.read().decode(errors="replace")[:300]}') from None
    ms = (time.perf_counter() - started) * 1000
    out = set()
    for f in (data.get('Result') or {}).get('Files') or []:
        for m in f.get('LineMatches') or []:
            out.add(f"{f['FileName']}:{m['LineNumber']}")
    return out, ms


def rg_search(q, cwd):
    started = time.perf_counter()
    r = subprocess.run(rg_args(q), cwd=cwd, capture_output=True, text=True, errors='replace')
    return rg_lines(r.stdout), (time.perf_counter() - started) * 1000


def size_of(folder):
    return sum(os.path.getsize(os.path.join(d, f)) for d, _, fs in os.walk(folder) for f in fs)


def start_server(index_dir):
    proc = subprocess.Popen([f'{E}/bin/zoekt-webserver', '-index', index_dir, '-listen', f'127.0.0.1:{PORT}', '-rpc'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    for _ in range(600):
        try:
            zoekt_search('case:yes "zzzzwarmup"')
            return proc
        except Exception:
            time.sleep(0.2)
    raise RuntimeError('zoekt-webserver did not start')


def main():
    queries_dir, out_file = sys.argv[1], sys.argv[2]
    probe = '--probe' in sys.argv
    reps = int(sys.argv[sys.argv.index('--reps') + 1]) if '--reps' in sys.argv else 3
    corpora = sys.argv[sys.argv.index('--corpora') + 1].split(',') if '--corpora' in sys.argv else CORPORA

    def read(name):
        with open(os.path.join(queries_dir, name)) as f:
            return [json.loads(l) for l in f if l.strip()]
    rows, builds = [], {}
    for corpus in corpora:
        repo = corpus.split('@')[0]
        src = f'{E}/corpora/{corpus}'
        idx = f'{E}/idx/{repo}'
        # Built from nothing every run, so the build time is a real one.
        shutil.rmtree(idx, ignore_errors=True)
        os.makedirs(idx)
        started = time.perf_counter()
        subprocess.run([f'{E}/bin/zoekt-index', '-index', idx, '-file_limit', '1000000000', src], capture_output=True)
        builds[repo] = {'buildMs': round((time.perf_counter() - started) * 1000), 'bytes': size_of(idx), 'corpusBytes': size_of(src)}
        print(repo, builds[repo], flush=True)
        server = start_server(idx)
        try:
            qs = [q for q in read('F2-exact.jsonl') if q['split'] == 'heldout' and q['repo'] == repo]
            qs += [{'family': 'F3', 'repo': repo, 'id': q['id'], 'pattern': q['name'], 'fixed': True, 'word': True} for q in read('F3-references.jsonl') if q['split'] == 'heldout' and q['repo'] == repo]
            if probe:
                for q in qs[:12]:
                    ref, _ = rg_search(q, src)
                    try:
                        a, _ = zoekt_search(zoekt_query(q, True))
                        b, _ = zoekt_search(zoekt_query(q, False))
                    except ValueError as e:
                        print('REFUSED', e)
                        continue
                    print(f"rg {len(ref):5} doubled {len(a):5} {'same' if a == ref else 'DIFF'}  single {len(b):5} {'same' if b == ref else 'DIFF'}  {zoekt_query(q, True)[:100]}")
                continue
            for n, q in enumerate(qs):
                zq = zoekt_query(q)
                order = ['rg', 'zoekt'] if n % 2 == 0 else ['zoekt', 'rg']
                times = {'rg': [], 'zoekt': []}
                sets = {}
                for tool in order:
                    for rep in range(reps + 1):
                        s, ms = rg_search(q, src) if tool == 'rg' else zoekt_search(zq)
                        if rep:
                            times[tool].append(ms)
                        sets[tool] = s
                rows.append({'id': q['id'], 'family': q['family'], 'repo': repo, 'rgMs': statistics.median(times['rg']), 'zoektMs': statistics.median(times['zoekt']), 'same': sets['rg'] == sets['zoekt'], 'rgHits': len(sets['rg']), 'zoektHits': len(sets['zoekt'])})
            print(repo, len(qs), 'queries', flush=True)
        finally:
            server.terminate()
            server.wait()
    if not probe:
        with open(out_file, 'w') as f:
            json.dump({'at': time.strftime('%Y-%m-%dT%H:%M:%S'), 'reps': reps, 'builds': builds, 'rows': rows}, f)
        print('wrote', out_file)


if __name__ == '__main__':
    main()

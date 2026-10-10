"""A finished run's R@1, R@5 and MRR on the tune half, on its validation fifth and on its fit
four fifths, by language, read from its results.jsonl, so an offline round can be put beside the
reference editor on the same queries (`task-2237`).

    python tools/completion-eval/rank-model/reference.py --run baseline-intellij-ml-on [--record <round folder>]
"""
import argparse
import json
import os
import sys
from collections import defaultdict

sys.path.insert(0, os.path.dirname(__file__))
from train import EVAL_ROOT, HERE, is_validation  # noqa: E402

def normalise(label):
    name = str(label).strip()
    for stop in ['(', '<', ' ', '!', ':', '?']:
        at = name.find(stop)
        if at > 0:
            name = name[:at]
    return name


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--run', required=True)
    ap.add_argument('--split', default='tune')
    ap.add_argument('--keys', help='a pools run whose queries to restrict to, comma separated runs')
    args = ap.parse_args()
    with open(os.path.join(HERE, '..', 'positions.json'), encoding='utf8') as f:
        positions = {p['id']: p for p in json.load(f)['positions']}
    keys = None
    if args.keys:
        keys = set()
        for run in args.keys.split(','):
            with open(os.path.join(EVAL_ROOT, 'runs', run, 'results.jsonl'), encoding='utf8') as f:
                keys.update(f"{json.loads(l)['id']}#{json.loads(l)['prefix']}" for l in f if l.strip())
    by = defaultdict(list)
    with open(os.path.join(EVAL_ROOT, 'runs', args.run, 'results.jsonl'), encoding='utf8') as f:
        for line in f:
            if not line.strip():
                continue
            r = json.loads(line)
            p = positions.get(r['id'])
            if not p or p['split'] != args.split:
                continue
            if keys is not None and f"{r['id']}#{r['prefix']}" not in keys:
                continue
            names = [normalise(l) for l in r.get('labels', [])]
            rank = names.index(p['expected']) + 1 if p['expected'] in names else 0
            part = 'validation' if is_validation(r['id']) else 'fit'
            for key in (f"{p['language']}|{part}", f"{p['language']}|{part}|{p['class']}", f"{p['language']}|{part}|p{r['prefix']}", f"{p['language']}|all"):
                by[key].append(rank)
    for key in sorted(by):
        ranks = by[key]
        n = len(ranks)
        print(f"{key:36s} n={n:6d}  R@1 {100*sum(r==1 for r in ranks)/n:5.1f}  R@5 {100*sum(0<r<=5 for r in ranks)/n:5.1f}  MRR {sum(1/r for r in ranks if r)/n:.3f}")


if __name__ == '__main__':
    main()

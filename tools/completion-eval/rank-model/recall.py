"""How often the expected name is in a run's pools at all, by language, class and prefix: the ceiling a
ranking of those pools can reach (`task-2237`).

    python tools/completion-eval/rank-model/recall.py --run <run with pools.jsonl> [--split tune]
"""
import argparse
import json
import os
import sys
from collections import defaultdict

sys.path.insert(0, os.path.dirname(__file__))
from train import HERE, read_pools  # noqa: E402


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--run', required=True)
    ap.add_argument('--split', default='tune')
    args = ap.parse_args()
    with open(os.path.join(HERE, '..', 'positions.json'), encoding='utf8') as f:
        positions = {p['id']: p for p in json.load(f)['positions']}
    by = defaultdict(lambda: [0, 0, 0])
    for q in read_pools(args.run, positions, args.split):
        p = q['position']
        names = [r['name'] for r in q['rows']]
        there = p['expected'] in names
        top = there and p['expected'] in [r['name'] for r in sorted(q['rows'], key=lambda r: r['at'])[:50]]
        for key in (f"{p['language']}|all", f"{p['language']}|{p['class']}", f"{p['language']}|p{q['prefix']}"):
            by[key][0] += 1
            by[key][1] += there
            by[key][2] += top
    for key in sorted(by):
        n, there, top = by[key]
        print(f'{key:24s} n={n:6d}  in the pool {100*there/n:5.1f}  in the first 50 {100*top/n:5.1f}')


if __name__ == '__main__':
    main()

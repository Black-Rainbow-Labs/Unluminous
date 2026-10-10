"""Trains the completion ranking model on the pools a window gathered, and scores pools offline
(`task-2237`, `tasks/task-2237-completion-learned-ranking-tdd.md` section 4.3).

    python tools/completion-eval/rank-model/train.py --run <run with pools.jsonl> [--out <model.json>]
        [--trees 300] [--leaves 31] [--rate 0.05] [--score-only <model.json>] [--split tune|held]

The pools come from `engine-unluminous.mjs --explain`. Only the tune half of the frozen positions is
trained on, and it is split again by a hash of the position id: four fifths to fit, one fifth to choose
the number of trees and to report. The order a model gives is the window's own: a server's preselected
row first, then the match class, then the score, then the name's bytes.
"""

import argparse
import hashlib
import json
import os
import sys
from collections import defaultdict

import numpy as np

sys.path.insert(0, os.path.dirname(__file__))
from features import CLASSES, FAR, NAMES, chain_rank, features  # noqa: E402

EVAL_ROOT = 'D:/unluminous-completion-eval'
HERE = os.path.dirname(os.path.abspath(__file__))


SCORED_BY_THE_CHAIN = 100
SCORED_BY_NEARNESS = 100


def scored_rows(rows):
    """`completion::learned_order`'s choice of the rows the model scores: the chain's first 100 and the
    100 written nearest the caret, by the nearer of the lines above and below, then the chain rank."""
    near = sorted(
        (min(FAR if r.get('linesAbove') is None else r['linesAbove'], FAR if r.get('linesBelow') is None else r['linesBelow']), chain_rank(r), i)
        for i, r in enumerate(rows)
        if r.get('linesAbove') is not None or r.get('linesBelow') is not None
    )[:SCORED_BY_NEARNESS]
    chosen = {i for _, _, i in near}
    return [chain_rank(r) < SCORED_BY_THE_CHAIN or i in chosen for i, r in enumerate(rows)]


def is_validation(position_id):
    """One position in five, by a hash of its id, is kept back to choose and report the model."""
    digest = hashlib.sha256(position_id.encode()).digest()
    return digest[0] % 5 == 0


def read_pools(runs, positions, split):
    """Every query of one run's pools, or several runs' separated by commas."""
    out = []
    for run in runs.split(','):
        out.extend(read_one_run(run, positions, split))
    # A run stopped between writing a pool and its result asks that query again on resuming; the last
    # pool written for a query is the one kept.
    last = {f"{q['id']}#{q['prefix']}": q for q in out}
    return list(last.values())


def read_one_run(run, positions, split):
    out = []
    path = os.path.join(EVAL_ROOT, 'runs', run, 'pools.jsonl')
    with open(path, encoding='utf8') as f:
        for line in f:
            if not line.strip():
                continue
            q = json.loads(line)
            p = positions.get(q['id'])
            if not p or (split != 'all' and p['split'] != split):
                continue
            q['position'] = p
            q['stem'] = ''.join(list(p['expected'])[:q['prefix']])
            out.append(q)
    return out


def vectors(q):
    lang = q['position']['language']
    tokens = q.get('tokens') or (0, 0)
    return [features(r, q['stem'], q['place'] or 'unknown', lang, tokens) for r in q['rows']]


def window_order(q, scores):
    """The rows' names in the order the window would give them with these scores."""
    rows = q['rows']
    scored = scored_rows(rows)
    def key(i):
        r = rows[i]
        cls = CLASSES.index(r['match'])
        preselected = r.get('preselect') and cls <= 1
        # `completion::learned_order`: an exact match is in the prefix group while the stem is short.
        return (0 if preselected else 1, max(cls, 1) if len(q['stem']) < 4 else cls, -scores[i] if scored[i] else float('inf'), chain_rank(r))
    return [rows[i]['name'] for i in sorted(range(len(rows)), key=key)]


def metrics(ranks):
    n = len(ranks)
    if not n:
        return None
    r1 = sum(1 for r in ranks if r == 1) / n
    r5 = sum(1 for r in ranks if 0 < r <= 5) / n
    mrr = sum(1 / r for r in ranks if r > 0) / n
    return n, r1, r5, mrr


def report(title, queries, orders):
    by = defaultdict(list)
    for q, names in zip(queries, orders):
        expected = q['position']['expected']
        rank = names.index(expected) + 1 if expected in names[:50] else 0
        lang = q['position']['language']
        for key in (f'{lang}|all', f"{lang}|{q['position']['class']}", f"{lang}|p{q['prefix']}"):
            by[key].append(rank)
    print(f'\n## {title}')
    for key in sorted(by):
        n, r1, r5, mrr = metrics(by[key])
        print(f'{key:28s} n={n:6d}  R@1 {100*r1:5.1f}  R@5 {100*r5:5.1f}  MRR {mrr:.3f}')
    return {k: metrics(v) for k, v in by.items()}


def record(folder, queries, orders, describe):
    """Writes one hill climb round: a row a query with its rank, R@1 and reciprocal rank."""
    os.makedirs(folder, exist_ok=True)
    with open(os.path.join(folder, 'results.jsonl'), 'w', encoding='utf8') as f:
        for q, names in zip(queries, orders):
            p = q['position']
            rank = names.index(p['expected']) + 1 if p['expected'] in names[:50] else 0
            row = {
                'prompt_id': f"{q['id']}#{q['prefix']}",
                'rep': 0,
                'prompt': f"{p['path']} {p['class']} prefix {q['prefix']}: {p['expected']}",
                'tags': [p['language'], p['class'], f"prefix{q['prefix']}"],
                'grade': {'r1': 1 if rank == 1 else 0, 'rr': 1 / rank if rank else 0, 'r5': 1 if 0 < rank <= 5 else 0},
                'model': 'n/a',
            }
            f.write(json.dumps(row) + '\n')
    with open(os.path.join(folder, 'summary.json'), 'w', encoding='utf8') as f:
        json.dump({'description': describe, 'target': 'code'}, f)


BLANKED = ['same_after']


def blanked(q):
    """The query as it would be if the caret were at the end of its line: nothing follows it, so no place
    is followed by what follows the caret."""
    copy = dict(q)
    copy['rows'] = [dict(r, sameAfter=0) for r in q['rows']]
    return copy


def build(queries, augment=False):
    if augment:
        queries = queries + [blanked(q) for q in queries]
    X, y, groups, kept = [], [], [], []
    for q in queries:
        names = [r['name'] for r in q['rows']]
        expected = q['position']['expected']
        if expected not in names:
            continue
        target = names.index(expected)
        short = len(q['stem']) < 4
        group = lambda r: max(CLASSES.index(r['match']), 1) if short else CLASSES.index(r['match'])  # noqa: E731
        cls = group(q['rows'][target])
        # Only the rows of the expected row's class compete with it: the class comes before the score.
        scored = scored_rows(q['rows'])
        if not scored[target]:
            continue
        idx = [i for i, r in enumerate(q['rows']) if group(r) == cls and scored[i]]
        if len(idx) < 2:
            continue
        vs = vectors(q)
        for i in idx:
            X.append(vs[i])
            y.append(1 if i == target else 0)
        groups.append(len(idx))
        kept.append(q)
    return np.array(X, dtype=np.float64), np.array(y), groups, kept


def score_with(model, queries):
    orders = []
    for q in queries:
        vs = np.array(vectors(q), dtype=np.float64) if q['rows'] else np.zeros((0, len(NAMES)))
        scores = model.predict(vs) if len(vs) else []
        orders.append(window_order(q, list(scores)))
    return orders


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--run', required=True)
    ap.add_argument('--out')
    ap.add_argument('--trees', type=int, default=400)
    ap.add_argument('--leaves', type=int, default=31)
    ap.add_argument('--depth', type=int, default=-1)
    ap.add_argument('--rate', type=float, default=0.05)
    ap.add_argument('--min-leaf', type=int, default=50)
    ap.add_argument('--split', default='tune')
    ap.add_argument('--drop', default='', help='features to leave out, comma separated')
    ap.add_argument('--record', help='a hill climb round folder to write results.jsonl and summary.json into')
    ap.add_argument('--describe', default='', help="the round's one line description")
    ap.add_argument('--window', action='store_true', help='record the window order instead of the model')
    ap.add_argument('--augment', action='store_true', help='also fit every query as if nothing followed the caret')
    args = ap.parse_args()

    import lightgbm as lgb
    with open(os.path.join(HERE, '..', 'positions.json'), encoding='utf8') as f:
        frozen = json.load(f)
    positions = {p['id']: p for p in frozen['positions']}
    queries = read_pools(args.run, positions, args.split)
    fit = [q for q in queries if not is_validation(q['id'])]
    valid = [q for q in queries if is_validation(q['id'])]
    print(f'{len(queries)} queries: {len(fit)} to fit, {len(valid)} to validate')

    report('validation, the window order', valid, [[r['name'] for r in sorted(q['rows'], key=lambda r: r['at'])] for q in valid])

    if args.window:
        record(args.record, queries, [[r['name'] for r in sorted(q['rows'], key=lambda r: r['at'])] for q in queries], args.describe)
        return
    Xf, yf, gf, _ = build(fit, augment=args.augment)
    Xv, yv, gv, _ = build(valid)
    dropped = [NAMES.index(n) for n in args.drop.split(',') if n]
    if dropped:
        Xf[:, dropped] = 0
        Xv[:, dropped] = 0
    print(f'fit rows {len(yf)} groups {len(gf)}; validation rows {len(yv)} groups {len(gv)}')
    params = {
        'objective': 'lambdarank',
        'metric': 'ndcg',
        'ndcg_eval_at': [1, 5],
        'learning_rate': args.rate,
        'num_leaves': args.leaves,
        'max_depth': args.depth,
        'min_data_in_leaf': args.min_leaf,
        'lambdarank_truncation_level': 20,
        'feature_fraction': 0.9,
        'bagging_fraction': 0.9,
        'bagging_freq': 1,
        'seed': 2237,
        'deterministic': True,
        'force_row_wise': True,
        'verbose': -1,
        'num_threads': 8,
    }
    train = lgb.Dataset(Xf, yf, group=gf, feature_name=NAMES)
    val = lgb.Dataset(Xv, yv, group=gv, reference=train)
    model = lgb.train(params, train, args.trees, valid_sets=[val], valid_names=['validation'],
                      callbacks=[lgb.early_stopping(60, verbose=False), lgb.log_evaluation(100)])
    print('best iteration', model.best_iteration)
    report('validation, the model', valid, score_with(model, valid))
    blank = [blanked(q) for q in valid]
    report('validation with nothing after the caret, the window order', blank, [[r['name'] for r in sorted(q['rows'], key=lambda r: r['at'])] for q in blank])
    report('validation with nothing after the caret, the model', blank, score_with(model, blank))
    importance = sorted(zip(NAMES, model.feature_importance('gain')), key=lambda x: -x[1])
    print('\ngain:', ', '.join(f'{n} {g:.0f}' for n, g in importance[:20]))
    if args.record:
        record(args.record, queries, score_with(model, queries), args.describe)
    if args.out:
        dump = model.dump_model(num_iteration=model.best_iteration)
        with open(args.out, 'w', encoding='utf8') as f:
            json.dump({'features': NAMES, 'trees': [t['tree_structure'] for t in dump['tree_info']]}, f)
        print('wrote', args.out)


if __name__ == '__main__':
    main()

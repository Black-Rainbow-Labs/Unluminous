"""Writes a trained ranking model as Rust: `crates/unluminous-core/src/completion_model.rs` (`task-2237`).

    python tools/completion-eval/rank-model/export.py <model.json> [--checks <run>]

The trees become one flat array of nodes. A node with `feature < FEATURES` splits on that feature at
`threshold` (`<=` goes left); any other node is a leaf whose value is `threshold`. With `--checks <run>`
it also writes the feature rows and scores of a few hundred pool rows, which a core test scores again in
Rust and compares, so the code and the trainer cannot drift apart.
"""

import argparse
import json
import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(__file__))
from features import NAMES  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, '..', '..', '..'))
TARGET = os.path.join(ROOT, 'crates', 'unluminous-core', 'src', 'completion_model.rs')
CHECKS = os.path.join(ROOT, 'crates', 'unluminous-core', 'src', 'completion_model_checks.txt')


def flatten(tree, nodes):
    """Appends a tree's nodes depth first and answers the index of its root."""
    at = len(nodes)
    if 'leaf_value' in tree:
        nodes.append((65535, float(tree['leaf_value']), 0, 0))
        return at
    if tree['decision_type'] != '<=':
        raise SystemExit(f"a split this exporter does not know: {tree['decision_type']}")
    nodes.append(None)
    left = flatten(tree['left_child'], nodes)
    right = flatten(tree['right_child'], nodes)
    nodes[at] = (int(tree['split_feature']), float(tree['threshold']), left, right)
    return at


def predict(nodes, roots, x):
    total = 0.0
    for root in roots:
        at = root
        while nodes[at][0] != 65535:
            feature, threshold, left, right = nodes[at]
            at = left if x[feature] <= threshold else right
        total += nodes[at][1]
    return total


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('model')
    ap.add_argument('--checks')
    args = ap.parse_args()
    with open(args.model, encoding='utf8') as f:
        model = json.load(f)
    if model['features'] != NAMES:
        raise SystemExit('the model was trained on other features than features.py names')
    nodes, roots = [], []
    for tree in model['trees']:
        roots.append(flatten(tree, nodes))
    lines = [
        '//! The completion ranking model, written by `tools/completion-eval/rank-model/export.py` from a',
        '//! model trained on the tune half of `tools/completion-eval/positions.json`. Do not edit by hand;',
        '//! train and export again. `tasks/task-2237-completion-learned-ranking-tdd.md` says what it is.',
        '//!',
        '//! A node whose `feature` is below [`FEATURES`] splits on that feature at `threshold`, `<=` going',
        '//! `left`; any other node is a leaf, and its `threshold` is its value.',
        '',
        '/// One node of a tree.',
        '#[derive(Debug, Clone, Copy)]',
        'pub struct Node {',
        '    pub feature: u16,',
        '    pub threshold: f64,',
        '    pub left: u32,',
        '    pub right: u32,',
        '}',
        '',
        f'/// How many features a row is described by, in the order of `completion::Features`.',
        f'pub const FEATURES: usize = {len(NAMES)};',
        '',
        '/// The feature names, for `editor complete --explain` and the test that checks the trainer.',
        f'pub const NAMES: [&str; FEATURES] = [{", ".join(json.dumps(n) for n in NAMES)}];',
        '',
        '/// Where each tree starts in [`NODES`].',
        f'pub const ROOTS: [u32; {len(roots)}] = [{", ".join(str(r) for r in roots)}];',
        '',
        '/// Every node of every tree.',
        f'pub const NODES: [Node; {len(nodes)}] = [',
    ]
    for feature, threshold, left, right in nodes:
        lines.append(f'    Node {{ feature: {feature}, threshold: {threshold!r}, left: {left}, right: {right} }},')
    lines += [
        '];',
        '',
        '/// The model\'s score for a row\'s features, larger better.',
        '///',
        '/// @param x - the features',
        'pub fn score(x: &[f64; FEATURES]) -> f64 {',
        '    let mut total = 0.0;',
        '    for root in ROOTS {',
        '        let mut at = root as usize;',
        '        loop {',
        '            let node = &NODES[at];',
        '            if node.feature as usize >= FEATURES {',
        '                total += node.threshold;',
        '                break;',
        '            }',
        '            at = if x[node.feature as usize] <= node.threshold { node.left } else { node.right } as usize;',
        '        }',
        '    }',
        '    total',
        '}',
        '',
    ]
    text = '\n'.join(lines).replace('inf,', 'f64::INFINITY,')
    with open(TARGET, 'w', encoding='utf8', newline='\n') as f:
        f.write(text)
    print(f'wrote {TARGET}: {len(roots)} trees, {len(nodes)} nodes')
    if args.checks:
        from train import read_pools, vectors
        with open(os.path.join(HERE, '..', 'positions.json'), encoding='utf8') as f:
            positions = {p['id']: p for p in json.load(f)['positions']}
        queries = read_pools(args.checks, positions, 'tune')[::37][:200]
        rows = []
        for q in queries:
            for x in vectors(q)[:5]:
                rows.append({'x': x, 'score': predict(nodes, roots, x)})
        with open(CHECKS, 'w', encoding='utf8', newline='\n') as f:
            for row in rows:
                f.write(' '.join(repr(v) for v in [row['score']] + row['x']) + '\n')
        print(f'wrote {CHECKS}: {len(rows)} rows')


if __name__ == '__main__':
    main()

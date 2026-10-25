"""Make the data sets whose automatic axis scales are asked of Excel (tests/fixtures/charts/scale-cases.jsonl).

Run:  python -I make_scale_cases.py > scale-cases.jsonl

Each line is {"id", "values", "kind"} where kind is "linear" or "log". The sets cover small and large
numbers, narrow ranges far from zero, negatives, mixed signs, equal values, tiny and huge magnitudes.
"""
import json
import random

random.seed(2360)
cases = []


def add(values, kind="linear"):
    cases.append({"id": len(cases), "values": values, "kind": kind})


fixed = [
    [1, 2, 3, 4, 5],
    [3, 7, 2, 9, 5],
    [0, 10, 20, 30],
    [12, 18, 7, 25, 14, 30],
    [100, 200, 300],
    [500, 510, 505, 520, 515],
    [1000, 1001, 1002],
    [99.5, 100.2, 100.8, 101.1],
    [-5, -3, -8, -1],
    [-10, 10],
    [-2.5, 4.5, 1, -1],
    [5, 5, 5, 5],
    [0, 0, 0],
    [42],
    [-7],
    [0.001, 0.002, 0.0035],
    [0.00001, 0.00002, 0.00004],
    [1e6, 2.5e6, 3.1e6, 4e6],
    [1.2e9, 3.4e9, 2.2e9],
    [1e12, 5e12],
    [0.5, 0.75, 0.6, 0.9],
    [0.95, 0.97, 0.99],
    [1, 100, 10000],
    [2, 4, 8, 16, 32, 64, 128],
    [15, 18, 21, 24],
    [88, 92, 95, 97, 99],
    [0, 1],
    [1, 2],
    [-1, 1],
    [10, 11],
    [1234, 5678, 9012],
    [0.3, 0.1, 0.7, 0.2],
    [250, 260, 270, 280, 290, 300],
    [-100, -50, 0, 50, 100],
    [33, 66, 99],
    [7, 14, 21, 28, 35, 42, 49],
    [1.5, 2.5, 2.0, 3.0],
    [-0.5, -0.25, -0.75],
    [999, 1001],
    [0.1, 0.2, 0.3],
    [8, 9, 10, 11, 12, 13, 14, 15, 16, 17],
    [-1000, 500, 2500],
    [3.14159, 2.71828, 1.41421],
    [60, 65, 70, 75, 80],
    [1, 1000000],
    [0, 99],
    [0, 100],
    [0, 101],
    [20, 21, 22, 23, 24, 25],
    [450, 460, 470],
]
for v in fixed:
    add(v)
for _ in range(14):
    n = random.randint(3, 9)
    scale = 10 ** random.randint(-3, 7)
    offset = random.choice([0, 0, 1, 5, -3]) * scale
    add([round(offset + random.random() * scale * random.choice([1, 3, 10]), 6) for _ in range(n)])
for v in ([1, 10, 100, 1000], [2, 50, 3000], [0.01, 0.5, 20], [5, 6, 7], [1e-3, 1e3], [10, 20000]):
    add(v, "log")
for c in cases:
    print(json.dumps(c))

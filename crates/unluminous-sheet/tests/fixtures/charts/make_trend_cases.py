"""Make the data sets whose trendline labels are asked of Excel (tests/fixtures/charts/trend-cases.jsonl).

Run:  python -I make_trend_cases.py > trend-cases.jsonl
"""
import json
import math
import random

random.seed(2361)
cases = []


def add(x, y):
    cases.append({"id": len(cases), "x": x, "y": y})


add([1, 2, 3, 4, 5], [2, 4, 5, 4, 5])
add([1, 2, 3, 4, 5], [3.1, 4.9, 7.2, 8.8, 11.1])
add([1, 2, 3, 4, 5, 6], [12, 18, 7, 25, 14, 30])
add([1, 2, 3, 4], [2.718, 7.389, 20.086, 54.598])
add([1, 2, 3, 4, 5, 6, 7], [1, 8, 27, 64, 125, 216, 343])
add([10, 20, 30, 40, 50], [1.5, 2.9, 4.1, 6.2, 7.9])
add([0.5, 1, 1.5, 2, 2.5, 3], [0.3, 0.9, 1.4, 2.6, 3.1, 4.4])
add([1, 2, 3, 4, 5], [100, 220, 300, 390, 520])
add([1, 2, 3, 4, 5, 6, 7, 8], [5, 3, 8, 6, 9, 7, 11, 10])
add([1, 2, 3, 4, 5], [0.0001, 0.0003, 0.0002, 0.0005, 0.0004])
add([1, 2, 3, 4, 5], [1500000, 2600000, 3100000, 4800000, 5200000])
add([1, 2, 3, 4, 5, 6], [10, 9, 7, 4, 3, 1])
add([2, 4, 6, 8, 10], [1, 4, 9, 16, 25])
add([1, 2, 3, 4], [1, 1.0001, 1.0002, 1.0004])
add([1, 2, 3, 4, 5, 6], [-3, -1, 2, 4, 5, 9])
add([5, 15, 25, 35, 45, 55], [2, 3, 5, 4, 6, 8])
for n in (6, 9, 12):
    for scale in (1, 1000, 0.01):
        x = list(range(1, n + 1))
        y = [round(scale * (2 + 0.7 * i + random.random() * 3), 6) for i in x]
        add(x, y)
for c in cases:
    print(json.dumps(c))

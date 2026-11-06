# titles.py <titles.gz> <out.tsv>: every capitalized word in an English Wikipedia article title, written as
# word \t titles it appears in \t titles where it is not the first word \t zipf of its lowercase form.
# The spelling kept for a word is the one most titles use.
import gzip, re, sys, unicodedata
from collections import Counter, defaultdict
from wordfreq import zipf_frequency
tok = re.compile(r"^[^\W\d_]{3,}$")
def latin(w):
    return all('LATIN' in unicodedata.name(c, '') for c in w)
forms = defaultdict(Counter); mid = Counter()
with gzip.open(sys.argv[1], 'rt', encoding='utf-8', errors='replace') as f:
    next(f)
    for line in f:
        t = line.rstrip('\n')
        t = re.sub(r'_\(.*\)$', '', t)
        for i, w in enumerate(t.split('_')):
            w = w.strip(",.:;!?\"'()")
            if tok.match(w) and w[0].isupper() and latin(w):
                forms[w.lower()][w] += 1
                if i > 0:
                    mid[w] += 1
with open(sys.argv[2], 'w', encoding='utf-8', newline='') as o:
    for low, c in forms.items():
        w, n = c.most_common(1)[0]
        z = zipf_frequency(low, 'en')
        if z >= 1.0:
            o.write(f"{w}\t{n}\t{mid[w]}\t{z}\n")

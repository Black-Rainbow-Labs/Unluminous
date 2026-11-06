# supplement.py <candidates.tsv> <rejected-by-base.txt> <out> <T_low> <T_cap> <gap> [excluded.txt ...]
#
# Chooses the words added to SCOWL's en_US dictionary (task-2517). A word goes in when:
# - the base dictionary rejects it (rejected-by-base.txt, written by the spell_probe example);
# - it is a candidate: Wiktionary has a sense of it with none of the excluded tags, or it is a capitalized word in
#   English Wikipedia article titles (candidates.tsv, written by candidates.py and titles.py);
# - its wordfreq Zipf frequency is at least T_low when it starts lowercase, or T_cap when it starts with a capital
#   (a capitalized entry is only accepted capitalized, so it cannot hide a lowercase typo);
# - it is not a lowercase word shorter than MINLEN letters, and not a contraction written without its apostrophe
#   (theres, hes), which is a common word once the apostrophe is put back;
# - no word one edit away (a letter dropped, added, changed, or two swapped) is more than `gap` Zipf more common,
#   because that word is what a typo of it would most likely be. This is asked only of a word with no hyphen
#   that is rarer than FREE;
# - it is in none of the excluded lists, compared without case (British spellings, known misspellings);
# - it is not written in capitals, which the checker never underlines anyway.
import sys
from wordfreq import zipf_frequency, top_n_list
cand, rej, out, tlow, tcap, gap = sys.argv[1], sys.argv[2], sys.argv[3], float(sys.argv[4]), float(sys.argv[5]), float(sys.argv[6])
rejected = set(l.strip() for l in open(rej, encoding='utf-8'))
excluded = set()
for f in sys.argv[7:]:
    excluded |= set(l.strip() for l in open(f, encoding='utf-8'))
excluded_lower = {e.lower() for e in excluded}
common = {}
apostrophed = set()
for w in top_n_list('en', 60000):
    z = zipf_frequency(w, 'en')
    if z >= 3.0 and w.isalpha():
        common[w] = z
    if z >= 3.0 and ("'" in w or '’' in w):
        apostrophed.add(w.replace('’', "'").replace("'", ''))
# A lowercase word shorter than this is left out: short words are what a typo most often produces (de, ve, ot).
MINLEN = 5
# A word at least this common is not asked about the word one edit away: it is common in its own right (pilates
# against plates, wifi against wife), and the measurement in the TDD found the check cost more than it saved there.
FREE = 3.0
letters = 'abcdefghijklmnopqrstuvwxyz'
def edits(w):
    splits = [(w[:i], w[i:]) for i in range(len(w) + 1)]
    for a, b in splits:
        if b: yield a + b[1:]
        if len(b) > 1: yield a + b[1] + b[0] + b[2:]
        for c in letters:
            if b: yield a + c + b[1:]
            yield a + c + b
kept = []
for line in open(cand, encoding='utf-8'):
    w, z, src = line.rstrip('\n').split('\t')
    z = float(z)
    if w not in rejected or w in excluded or w.lower() in excluded_lower:
        continue
    if len(w) > 1 and w.isupper():
        continue
    cap = w[0].isupper()
    if z < (tcap if cap else tlow):
        continue
    lw = w.lower()
    if not cap and len(lw) < MINLEN:
        continue
    if lw in apostrophed:
        continue
    if gap < 99 and z < FREE and '-' not in lw and any(common.get(e, 0) > z + gap for e in edits(lw) if e != lw):
        continue
    kept.append(w)
open(out, 'w', encoding='utf-8', newline='').write(''.join(k + '\n' for k in sorted(set(kept))))
print(len(set(kept)))

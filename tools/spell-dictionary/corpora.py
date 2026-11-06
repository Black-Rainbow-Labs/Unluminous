# Writes misspellings.txt: one misspelled word a line from the Wikipedia list, Birkbeck and Norvig's sets,
# with the intended word beside it (tab separated).
import re, sys
d = sys.argv[1]
pairs = set()
for line in open(f'{d}/wp-misspellings.txt', encoding='utf-8'):
    m = re.match(r'^\s*([^\s-]+)->(.+)$', line)
    if m:
        for right in m.group(2).split(','):
            pairs.add((m.group(1).strip(), right.strip()))
right = None
for line in open(f'{d}/birkbeck.dat', encoding='latin-1'):
    line = line.strip()
    if line.startswith('$'):
        right = line[1:]
    elif line and right:
        pairs.add((line, right))
for f in ('norvig1.txt', 'norvig2.txt'):
    for line in open(f'{d}/{f}', encoding='utf-8'):
        if ':' in line:
            r, wrongs = line.split(':', 1)
            for w in wrongs.split():
                pairs.add((w.strip(), r.strip()))
ok = re.compile(r"^[A-Za-z]+$")
out = sorted((w, r) for w, r in pairs if ok.match(w) and w != r and w.lower() != r.lower())
open(sys.argv[2], 'w', encoding='utf-8', newline='').write(''.join(f'{w}\t{r}\n' for w, r in out))
print(len(out))

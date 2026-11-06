# Reads wikt.tsv and writes candidates.tsv: word \t zipf \t source for every word with at least one sense that
# carries none of the BAD tags (entry), the inflections listed on such an entry (form), and the capitalized words
# of a proper name written as several words, weighed by the frequency of the whole name (part).
import sys, re, unicodedata
from wordfreq import zipf_frequency
BAD = {'misspelling','nonstandard','proscribed','obsolete','archaic','dated','rare','dialectal','eye-dialect',
       'pronunciation-spelling','nonce-word','deliberate','Internet','slang','vulgar','offensive','derogatory',
       'colloquial','informal','humorous','abbreviation','initialism','acronym','clipping','obsolete-form'}
FORM_BAD = {'alternative','obsolete','archaic','dated','rare','nonstandard','misspelling','dialectal','romanization','error-unknown-tag'}
# A word, with an apostrophe inside it or hyphens joining its parts (Wi-Fi, T-shirt): the checker reads a
# hyphenated word whole when the dictionary has it, and its parts one at a time otherwise.
ok = re.compile(r"^[^\W\d_]+(?:['’][^\W\d_]+)?(?:-[^\W\d_]+)*$")
def latin(w):
    return all(('LATIN' in unicodedata.name(c, '')) or c in "'’-" for c in w)
entries, forms, parts = {}, {}, {}
for line in open(sys.argv[1], encoding='utf-8'):
    word, pos, senses, fms = line.rstrip('\n').split('\t')
    clean = any(not (set(s.split(',')) & BAD) for s in senses.split('|'))
    if pos == 'name' and clean and ' ' in word:
        z = zipf_frequency(word.lower(), 'en')
        for p in word.split(' '):
            if p[:1].isupper() and ok.match(p) and latin(p) and len(p) > 2:
                parts[p] = max(parts.get(p, 0), z)
        continue
    if not ok.match(word) or not latin(word) or len(word) < 2:
        continue
    if clean:
        entries[word] = True
        for f in fms.split('|') if fms else []:
            fw, _, ft = f.rpartition('#')
            if fw and ok.match(fw) and latin(fw) and not (set(ft.split(',')) & FORM_BAD):
                forms.setdefault(fw, True)
    else:
        entries.setdefault(word, False)
with open(sys.argv[2], 'w', encoding='utf-8', newline='') as o:
    for w, clean in entries.items():
        if clean:
            o.write(f"{w}\t{zipf_frequency(w.lower(),'en')}\tentry\n")
    for w in forms:
        if w not in entries:
            o.write(f"{w}\t{zipf_frequency(w.lower(),'en')}\tform\n")
    for w, z in parts.items():
        if w not in entries and w not in forms:
            o.write(f"{w}\t{z}\tpart\n")

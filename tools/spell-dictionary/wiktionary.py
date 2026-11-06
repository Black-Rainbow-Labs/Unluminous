# Streams the kaikki Wiktionary English extract and writes one line per (word, pos):
# word \t pos \t tags of every sense joined by | (sense tags comma separated) \t forms (inflections) joined by |
import gzip, json, sys
src, out = sys.argv[1], sys.argv[2]
n = 0
with gzip.open(src, 'rt', encoding='utf-8') as f, open(out, 'w', encoding='utf-8', newline='') as o:
    for line in f:
        e = json.loads(line)
        if e.get('lang_code') != 'en':
            continue
        word = e.get('word', '')
        if not word or '\t' in word or '\n' in word:
            continue
        senses = []
        for s in e.get('senses', []):
            tags = set(s.get('tags', []))
            if s.get('form_of') or s.get('alt_of'):
                tags.add('FORM' if s.get('form_of') else 'ALT')
            senses.append(','.join(sorted(tags)))
        forms = []
        for fm in e.get('forms', []):
            t = fm.get('tags', [])
            if any(x in t for x in ('table-tags', 'inflection-template', 'romanization')):
                continue
            w = fm.get('form', '')
            if w and '\t' not in w and '|' not in w:
                forms.append(w + '#' + ','.join(t))
        o.write(f"{word}\t{e.get('pos','')}\t{'|'.join(senses)}\t{'|'.join(forms)}\n")
        n += 1
print(n)

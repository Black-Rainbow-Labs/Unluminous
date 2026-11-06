#!/bin/bash
# Rebuilds crates/unluminous-word/dictionaries/en_US-extra.dic.deflate, the words added to SCOWL's en_US
# dictionary (task-2517). Needs python with `pip install wordfreq`, curl, unzip, and about 1 GB of scratch.
#   bash tools/spell-dictionary/build.sh <scratch folder>
# It downloads the Wiktionary extract (kaikki.org, 534 MB), the English Wikipedia title list (110 MB), SCOWL's
# en_GB dictionaries and the misspelling corpora, and prints the measurements the TDD records.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
work=${1:?scratch folder}
mkdir -p "$work"
win() { if command -v cygpath >/dev/null; then cygpath -m "$1"; else echo "$1"; fi; }
W=$(win "$work"); H=$(win "$here"); D=$(win "$repo/crates/unluminous-word/dictionaries")
scowl=https://github.com/en-wl/wordlist/releases/download/rel-2026.02.25
get() { [ -s "$work/$2" ] || curl -sfL -o "$work/$2" "$1"; }
get https://kaikki.org/dictionary/English/kaikki.org-dictionary-English.jsonl.gz wiktionary.jsonl.gz
get https://dumps.wikimedia.org/enwiki/latest/enwiki-latest-all-titles-in-ns0.gz titles.gz
get "https://en.wikipedia.org/w/index.php?title=Wikipedia:Lists_of_common_misspellings/For_machines&action=raw" wp-misspellings.txt
get https://www.dcs.bbk.ac.uk/~ROGER/missp.dat birkbeck.dat
get https://norvig.com/spell-testset1.txt norvig1.txt
get https://norvig.com/spell-testset2.txt norvig2.txt
for v in ise ize; do
  get "$scowl/hunspell-en_GB-$v-2026.02.25.zip" "gb-$v.zip"
  unzip -qo "$work/gb-$v.zip" -d "$work/gb-$v"
  tr -d '\r' < "$work/gb-$v/en_GB-$v.aff" > "$work/gb_$v.aff"
  tr -d '\r' < "$work/gb-$v/en_GB-$v.dic" > "$work/gb_$v.dic"
done
cargo build -q --release --manifest-path "$repo/Cargo.toml" -p unluminous-word --example spell_probe
probe="$repo/target/release/examples/spell_probe"
[ -x "$probe" ] || probe="$probe.exe"
# The base dictionary alone, so the supplement is chosen against what ships without it.
export UNLUMINOUS_SPELL_AFF=$D/en_US.aff
python -c "import zlib,sys;open(sys.argv[2],'wb').write(zlib.decompress(open(sys.argv[1],'rb').read(),-15))" "$D/en_US.dic.deflate" "$W/base.dic"
export UNLUMINOUS_SPELL_DIC=$W/base.dic
python "$H/wiktionary.py" "$W/wiktionary.jsonl.gz" "$W/wikt.tsv"
python "$H/candidates.py" "$W/wikt.tsv" "$W/wikt_cands.tsv"
python "$H/titles.py" "$W/titles.gz" "$W/titles.tsv"
awk -F'\t' '$2>=3 && $4>=1.5 {print $1"\t"$4"\ttitle"}' "$work/titles.tsv" > "$work/title_cands.tsv"
cat "$work/wikt_cands.tsv" "$work/title_cands.tsv" > "$work/cands.tsv"
awk -F'\t' '$2>=1.5 {print $1}' "$work/cands.tsv" | sort -u > "$work/cand_words.txt"
"$probe" --rejected --file "$W/cand_words.txt" > "$work/rejected.txt"
# British spellings: rejected by en_US and accepted by either en_GB dictionary.
for v in ise ize; do
  UNLUMINOUS_SPELL_AFF=$W/gb_$v.aff UNLUMINOUS_SPELL_DIC=$W/gb_$v.dic "$probe" --rejected --file "$W/rejected.txt" | sort -u > "$work/gb_rejected_$v.txt"
done
comm -23 <(sort -u "$work/rejected.txt") <(comm -12 "$work/gb_rejected_ise.txt" "$work/gb_rejected_ize.txt") > "$work/british.txt"
grep -o '^ *[^ -]*->' "$work/wp-misspellings.txt" | sed 's/->//; s/^ *//' | sort -u > "$work/known_misspellings.txt"
python "$H/supplement.py" "$W/cands.tsv" "$W/rejected.txt" "$W/extra.dic" 2.0 1.5 1.0 "$W/british.txt" "$W/known_misspellings.txt"
python -c "import zlib,sys;c=zlib.compressobj(9,zlib.DEFLATED,-15);d=open(sys.argv[1],'rb').read();open(sys.argv[2],'wb').write(c.compress(d)+c.flush())" "$W/extra.dic" "$D/en_US-extra.dic.deflate"
# Measure: held-out misspellings (Birkbeck and Norvig, less the Wikipedia list the build excludes) still flagged.
python "$H/corpora.py" "$W" "$W/misspellings.tsv"
awk -F'\t' 'NR==FNR{wp[$1];next} !($1 in wp){print $1}' "$work/known_misspellings.txt" "$work/misspellings.tsv" > "$work/heldout.txt"
before=$("$probe" --rejected --file "$W/heldout.txt" 2>&1 >/dev/null)
after=$(UNLUMINOUS_SPELL_EXTRA=$W/extra.dic "$probe" --rejected --file "$W/heldout.txt" 2>&1 >/dev/null)
echo "words added: $(wc -l < "$work/extra.dic")"
echo "held-out misspellings flagged, base: $before"
echo "held-out misspellings flagged, with the added words: $after"

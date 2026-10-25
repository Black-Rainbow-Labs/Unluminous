# thesaurus.txt.gz

Synonyms for the Synonyms entry on the Word right click menu, read by `src/thesaurus.rs`.

Source: Princeton WordNet 3.1, `https://wordnetcode.princeton.edu/wn3.1.dict.tar.gz` (the `dict` folder).
WordNet's licence is in `WORDNET-LICENSE.txt`. The download has no separate licence file, so the file is
the licence text at the top of each `data.*` file, with the line numbers removed.

What the script did: it read `data.noun`, `data.verb`, `data.adj` and `data.adv` for each synset's words,
and `index.<pos>` for the order of a word's synsets, which is WordNet's order of how common each sense is.
For each word and part of speech it kept the first 6 synsets, each as at most 8 other words. Words and
synonyms are lower case with underscores turned into spaces, and anything longer than 3 words or with
non ASCII letters is left out. The `*.exc` files became an `#exceptions` section mapping inflected forms
(ran, geese, better) to their base word. Lines are `word<TAB>pos<TAB>synset|synset...<TAB>...`, sorted by
word, gzip level 9.

The script is outside the repository, at `D:/agent-worktrees/task-2359-out/build_thesaurus.py`. To rebuild,
extract the archive into an empty folder and run
`python -I build_thesaurus.py <folder>/dict thesaurus.txt.gz [max synsets] [max words]`.

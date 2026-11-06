en_US-extra.dic.deflate
=======================

56,542 words added to SCOWL's en_US Hunspell dictionary (en_US.dic.deflate, license in LICENSE-en_US.txt)
for Unluminous's Page spell check (task-2517). It is raw deflate, one word a line, with no affix flags.
`tools/spell-dictionary/build.sh` makes it again and tasks/task-2517-spell-check-tdd.md says how each word
was chosen and what that was measured to cost.

The words come from these sources:

- Wiktionary, through the kaikki.org English extract (https://kaikki.org/dictionary/English/).
  Wiktionary's text is available under the Creative Commons Attribution-ShareAlike 4.0 license
  (https://creativecommons.org/licenses/by-sa/4.0/). Only headwords and their inflections are used.
- The titles of English Wikipedia articles (https://dumps.wikimedia.org/enwiki/latest/), also under
  CC BY-SA 4.0. Only the capitalized words of the titles are used, with the capitalization most titles give.
- Word frequencies from wordfreq by Robyn Speer (https://github.com/rspeer/wordfreq). Its data is
  available under CC BY-SA 4.0. The frequencies are used to choose words and are not in the file.
- Exclusions: SCOWL's en_GB dictionaries (same license as SCOWL), and Wikipedia's list of common
  misspellings (https://en.wikipedia.org/wiki/Wikipedia:Lists_of_common_misspellings/For_machines,
  CC BY-SA 4.0).

This word list is a compilation of those sources and is distributed under CC BY-SA 4.0, separately from
the Apache 2.0 license that covers Unluminous's code.

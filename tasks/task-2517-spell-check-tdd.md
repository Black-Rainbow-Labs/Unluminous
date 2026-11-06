# task-2517: Page spell check accepts places, brands and recent words

> "Our spell check in Page/word needs improvement. It gets common places like Dolomites identified as error,
> pilates etc. Improve our spell check. Research online, write a tdd, and implement."

## 1. What was wrong

The Page (Word) plugin checks spelling with `spellbook` over SCOWL's en_US Hunspell dictionary, version
2020.12.07, SCOWL size 60. Size 60 is the list LibreOffice and Firefox ship. It is deliberately small, so that
an uncommon word does not hide a typo of a common one, and it has few proper names. Measured with the new
`spell_probe` example, it rejected:

| Kind | Rejected by the 2020 dictionary |
|---|---|
| Places | Dolomites, Phuket, Mykonos, Santorini, Amalfi, Positano, Machu Picchu |
| Brands | Spotify, CrossFit, Airbnb, Zumba |
| Recent words | pilates, kombucha, ramen, matcha, burpee, kettlebell, vlog, wifi, Wi-Fi |

On 407,550 words of English Wikipedia text about travel, fitness, food and technology (73 articles, the
`articles.py` corpus), it rejected 24,879 words.

## 2. What was considered

| Option | What it is | Result |
|---|---|---|
| SCOWL size 70 (`en_US-large`) | The next SCOWL size up, 77,000 stems | Rejected. Beyond what the 2026 size 60 list already accepts, it adds only `wifi` from section 1, and it stops underlining 468 more of the misspellings in the corpora below. SCOWL's own README warns that it holds words such as `ort` and `calender` that are likely typos. |
| SCOWL 2026.02.25, size 60 | The current release of the same list | Taken as the base. It accepts `pilates`, `ramen` and `vlog`, and its affix file treats `’` as an apostrophe. |
| A word frequency list alone (wordfreq, OpenSubtitles) | Every word seen often enough | Rejected. Misspellings are as frequent as many real words: `alot` has a Zipf frequency of 3.93, `teh` 3.04, `definately` 2.87, against `dolomites` 2.49. Frequency cannot tell them apart, and the lists are lowercase, so they lose the capital on a name. |
| Wiktionary (kaikki.org extract) | 1.5 million English entries with part of speech, sense tags and inflections | Used. Wiktionary tags a misspelling as `misspelling`, and tags `nonstandard`, `obsolete`, `rare` and so on, so a real word can be told from a common error. It has Dolomites, Phuket, Mykonos, kombucha, burpee and kettlebell. It has almost no trademarks, by its own policy. |
| English Wikipedia article titles | 17 million titles | Used, for capitalized words only. They supply the brands and the smaller places Wiktionary lacks (Spotify, CrossFit, Positano, Machu, Picchu), with the capitals most titles use. |
| GeoNames | A gazetteer of 12 million places | Not used. Wikipedia titles cover the places people write about, and GeoNames' small places are mostly names nobody types, each of which could hide a typo. |
| A language model or an online service | Ask a model or a server | Rejected. Spell check runs on every frame a paragraph changes, must work offline, and sends no document text anywhere. |

## 3. The design

### 3.1 The added words

`crates/unluminous-word/dictionaries/en_US-extra.dic.deflate` holds 56,542 words, one a line with no affix
flags, deflated to 205,901 bytes. `Speller::english` inflates SCOWL's list and this one and gives both to
`spellbook` as one `.dic`. `tools/spell-dictionary/build.sh` makes the file again from the sources. A word goes
in when all of these are true:

1. SCOWL's en_US list rejects it.
2. Wiktionary has a sense of it with none of these tags: misspelling, nonstandard, proscribed, obsolete,
   archaic, dated, rare, dialectal, eye dialect, pronunciation spelling, nonce word, deliberate, Internet,
   slang, vulgar, offensive, derogatory, colloquial, informal, humorous, abbreviation, initialism, acronym,
   clipping. An inflection listed on such an entry counts too, and so does each capitalized word of a
   proper name written as several words (Machu Picchu). Or it is a capitalized word in at least three
   English Wikipedia titles.
3. Its wordfreq Zipf frequency is at least 2.0 when it starts lowercase, or 1.5 when it starts with a
   capital. Hunspell accepts a capitalized entry only capitalized or in capitals, so `Phuket` is accepted
   and `phuket` is still underlined, and a name cannot hide a lowercase typo.
4. A lowercase word has at least five letters, and is not a contraction written without its apostrophe
   (`theres`, `hes`, `lookin`).
5. No word one edit away (a letter dropped, added, changed, or two swapped) is more than 1.0 Zipf more
   common. That word is what a typo of the candidate would most likely be: `calender` is left out because
   of `calendar`, and `matcha` because of `match`. A word with a Zipf frequency of 3.0 or more is not asked
   this, because it is common in its own right (`pilates` against `plates`).
6. It is not a British spelling, which is a word SCOWL's en_GB lists accept and its en_US list rejects
   (`colour`, `organise`, `centre`). Word in United States English underlines those too.
7. It is not on Wikipedia's list of common misspellings (4,306 words).
8. It is not written in capitals, which the checker never underlines.

### 3.2 Two changes to the checker

- **A possessive is correct when its word is.** `Speller::is_correct("Spotify's")` asks for `Spotify` when
  the whole word is not known. SCOWL's own entries carry the possessive as an affix flag; the added words and
  the words a person adds carry none.
- **A hyphenated word the dictionary has whole is one word.** `check_text` joins a word to the words after it
  across `-` or U+2011, the non-breaking hyphen, and accepts the whole when the dictionary has it (`Wi-Fi`,
  `T-shirt`, 5,800 such words in the added list). Otherwise each part is checked on its own, as before and as
  Word does with `well-known`.

`Speller::from_hunspell(affixes, words)` builds a speller over any Hunspell dictionary, which the probe uses to
measure one dictionary against another.

## 4. Measurements

Three misspelling corpora: Wikipedia's list of common misspellings, the Birkbeck spelling error corpus, and
Peter Norvig's two test sets, 38,844 distinct misspelled words. The Wikipedia list is excluded by the build, so
the fair measure of what the added words hide is the other two, 34,197 words, called held out below. A
misspelling that is itself a real word (`there` for `their`) cannot be caught by any dictionary and is in
neither count.

| Dictionary | Held out misspellings underlined | Wikipedia text words underlined |
|---|---|---|
| SCOWL 2020 size 60 (before) | 30,510 | 24,879 |
| SCOWL 2026 size 60 | 30,521 | 23,536 |
| SCOWL 2026 size 70 (`en_US-large`) | 30,087 | 19,576 |
| SCOWL 2026 size 60 and the added words (after) | 30,490 | 15,887 |

So the change underlines 36% fewer words in real text and stops underlining 20 of 30,510 held out
misspellings, 0.07%. The larger SCOWL list underlines fewer words in real text than before, but it stops
underlining 423 held out misspellings, 21 times as many.

The thresholds in 3.1 were chosen by sweeping them against these two numbers:

| Rule changed | Words added | Held out underlined | Text underlined |
|---|---|---|---|
| Edit check on every word, no length limit | 55,640 | 30,494 | 16,781 |
| No edit check at all | 47,663 | 29,898 | 14,651 |
| Edit check skipped at Zipf 3.0, no length limit | 57,424 | 30,275 | 14,962 |
| Edit check skipped at Zipf 3.0, five letters (chosen) | 56,542 | 30,490 | 15,887 |
| Chosen, and four letter words at Zipf 3.5 | 56,655 | 30,468 | 15,874 |

Starting the probe and reading the dictionary went from about 13 ms to 24 ms on this machine. The window reads
it once a session, the first time a document is checked.

## 5. What is still underlined, and why

- **`matcha` and `wifi`**: `matcha` is one edit from `match`, which is 2.5 Zipf more common, and `wifi` is
  under five letters. Each rule that would let them in let in about 20 more misspellings (section 4). `Wi-Fi`
  is accepted, and `wifi` suggests it.
- **Surnames and foreign names** that Wikipedia titles rarely use, such as `Lizárraga` or `Pogačar`.
- **British spellings in an English (United Kingdom) run**: the checker has only the en_US dictionary, so a run
  marked `en-GB` is checked as American English. Word would use its UK dictionary. See section 7.
- **Words in a run whose language comes from its style or the document defaults** are checked as English
  whatever the language is, because `checked_text` reads only the run's own `w:lang`. See section 7.

## 6. Licenses

SCOWL is under its own permissive license (`LICENSE-en_US.txt`). The added words are a compilation of
Wiktionary headwords and Wikipedia title words, chosen with wordfreq's frequencies, all under CC BY-SA 4.0.
`dictionaries/README-en_US-extra.txt` attributes each source and says the list is distributed under
CC BY-SA 4.0, separately from the Apache 2.0 license on the code.

## 7. Follow ups, not done in this ticket

- **A dictionary per English variant.** SCOWL ships en_GB, en_CA and en_AU lists of the same shape; the added
  words in 3.1 work with any of them, since British spellings were only excluded for the American list.
  `check_text` would need the language of each range of the paragraph, and the cache in
  `services/word/spelling.rs` would need the language in its key.
- **The effective language of a run**: its own `w:lang`, then its paragraph style's, then the document
  defaults, the way `Session::language_at_caret` already works it out.

## 8. How to rebuild and measure

```sh
pip install wordfreq
bash tools/spell-dictionary/build.sh <scratch folder>       # downloads about 650 MB, writes the .deflate
cargo run --release -p unluminous-word --example spell_probe -- Dolomites matcha recieve
cargo run --release -p unluminous-word --example spell_probe -- --rejected --file <words.txt>
```

`build.sh` prints the held out count before and after. `articles.py` fetches the Wikipedia text corpus.
`UNLUMINOUS_SPELL_AFF`, `UNLUMINOUS_SPELL_DIC` and `UNLUMINOUS_SPELL_EXTRA` point the probe at other files.

Tests: `crates/unluminous-word/tests/spelling.rs` has
`places_brands_and_recent_words_are_not_underlined`, `common_misspellings_are_still_underlined`,
`an_added_name_is_only_accepted_capitalized`, `a_possessive_is_correct_when_its_word_is` and
`a_hyphenated_word_is_read_whole_when_the_dictionary_has_it_and_by_its_parts_otherwise`.

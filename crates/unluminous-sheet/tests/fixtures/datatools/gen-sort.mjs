// Writes sort-text.json, the scenarios about the order Excel gives text and mixed values. Run it with node,
// then run tools/excel-oracle/data-tools.ps1 on the result to get sort-text.excel.json.
import fs from 'node:fs';

const out = [];
const here = new URL('./', import.meta.url);

function column(name, values) {
  const cells = { A1: 'h', B1: 'i' };
  values.forEach((v, i) => {
    cells['A' + (i + 2)] = v;
    cells['B' + (i + 2)] = i;
  });
  return { name, sheets: { Sheet1: cells } };
}
const sortOp = (range, key, order, extra = {}) => ({
  kind: 'sort', range, header: 'yes', levels: [{ key, order }], ...extra,
});

// Text in the order Excel sorts it: punctuation, symbols, digits, then letters; accents; ligatures; case.
const texts = ['apple', 'Banana', 'cherry', 'Apple', 'banana', "'_x", "'-y", "''z", "'1abc", "'10", "'9", ' lead', 'a b', 'ab', 'a-b', "a'b",
  'résumé', 'resume', 'Resume', 'Éa', 'eb', 'ZZ', 'zz', 'α', '日本', '~t', '#h', '@a', '$d', "'1st", 'a1', 'a10', 'a2', 'A1', 'co-op', 'coop', 'co op',
  '!bang', '"q', '%p', '&amp', '(par', ')par', '*star', ',comma', '.dot', '/slash', ':colon', ';semi', '<lt', '>gt', '?q', '[br', '\\bs', ']br', '^car',
  '`bt', '{cb', '|bar', '}cb', "'0", "'00", "'-1", "'1.5", 'ß', 'ss', 'straße', 'strasse', 'æ', 'ae', 'ø', 'o', 'ö', 'oe', 'ñ', 'n', 'ç', 'c', 'd', 'Z', 'z', 'a', 'A',
  "'+plus", "'=eq", 'àb', 'ab', 'áb', 'âb', 'ãb', 'äb', 'åb', 'ēb', 'ăb', 'ąb', 'ǎb', 'çb', 'čb', 'đb', 'ěb', 'ğb', 'ıb', 'ib', 'İb', 'ķb', 'łb', 'ńb', 'őb', 'řb', 'šb', 'ţb', 'űb', 'žb'];
for (const order of ['asc', 'desc']) {
  const range = `A1:B${texts.length + 1}`;
  out.push({ ...column('text-' + order, texts), op: sortOp(range, 'A2', order) });
  out.push({ ...column('text-case-' + order, texts), op: sortOp(range, 'A2', order, { matchCase: true }) });
}
const mixed = [3, -1, 0, 2.5, 100, 'x', 'Y', '=TRUE', '=FALSE', '=NA()', '=1/0', '=""', '1/2/2024', '1/1/2023', "'12", 0.00001, 1e300, 'b', ''];
for (const order of ['asc', 'desc']) out.push({ ...column('mixed-' + order, mixed), op: sortOp(`A1:B${mixed.length + 1}`, 'A2', order) });

fs.writeFileSync(new URL('./sort-text.json', here), JSON.stringify(out));
console.log(out.length, 'sort scenarios');

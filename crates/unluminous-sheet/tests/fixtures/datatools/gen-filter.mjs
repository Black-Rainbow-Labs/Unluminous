import fs from 'node:fs';
const today = new Date();
const serialOf = (d) => Math.floor((Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()) - Date.UTC(1899, 11, 30)) / 86400000);
const dateText = (d) => `${d.getMonth() + 1}/${d.getDate()}/${d.getFullYear()}`;
const plus = (days) => { const d = new Date(today.getFullYear(), today.getMonth(), today.getDate() + days); return d; };
const names = ['apple', 'Banana', 'cherry', 'avocado', 'blueberry', null, "'5", 'Apple pie', 'a*b', 'ant', 'APPLE', 'grape'];
const qty = [5, 3, 10, 7, 3, 0, 'n/a', 7.5, -2, 5, 8, null];
const dates = [plus(0), plus(-1), plus(1), plus(-8), plus(8), plus(-35), plus(35), plus(-100), plus(100), plus(-400), plus(400), plus(-2)];
function base(extra = {}) {
  const cells = { A1: 'Name', B1: 'Qty', C1: 'Date', D1: 'Note' };
  names.forEach((n, i) => { if (n !== null) cells['A' + (i + 2)] = n; });
  qty.forEach((q, i) => { if (q !== null) cells['B' + (i + 2)] = q; });
  dates.forEach((d, i) => { cells['C' + (i + 2)] = dateText(d); });
  cells.D2 = 'x'; cells.D4 = 'y';
  return { sheets: { Sheet1: cells }, formats: [{ range: 'C2:C13', numfmt: 'm/d/yyyy' }], ...extra };
}
const R = 'A1:D13';
const out = [];
const filterOp = (field, o = {}) => ({ kind: 'filter', range: R, field, ...o });
let n = 0;
const add = (name, op, extra) => { out.push({ name, ...base(extra), op }); n++; };
for (const [label, c1] of [
  ['eq-apple', 'apple'], ['eq-apple-eq', '=apple'], ['ne-apple', '<>apple'], ['contains-an', '=*an*'], ['begins-a', '=a*'], ['ends-e', '=*e'],
  ['notcontains-an', '<>*an*'], ['notbegins-a', '<>a*'], ['notends-e', '<>*e'], ['qmark', '=a?ple'], ['tilde-star', '=a~*b'], ['gt-m', '>m'], ['ge-b', '>=b'], ['lt-b', '<b'],
  ['blank', '='], ['nonblank', '<>'], ['star', '=*'], ['five-text', '=5'], ['lt-apple', '<apple'],
]) add('name-' + label, filterOp(1, { criteria1: c1 }));
for (const [label, c1] of [
  ['gt5', '>5'], ['ge5', '>=5'], ['lt5', '<5'], ['le5', '<=5'], ['eq5', '=5'], ['eq5-plain', '5'], ['ne5', '<>5'], ['eq0', '=0'], ['blank', '='], ['nonblank', '<>'], ['gt-neg', '>-1'],
  ['text-na', '=n/a'], ['eq-7.5', '=7.5'], ['star', '=*'], ['contains-7', '=*7*'], ['gt-text', '>a'],
]) add('qty-' + label, filterOp(2, { criteria1: c1 }));
add('qty-and', filterOp(2, { criteria1: '>3', operator: 1, criteria2: '<8' }));
add('qty-or', filterOp(2, { criteria1: '<3', operator: 2, criteria2: '>8' }));
add('name-or-text', filterOp(1, { criteria1: '=apple', operator: 2, criteria2: '=cherry' }));
add('name-and-text', filterOp(1, { criteria1: '=*a*', operator: 1, criteria2: '<>*e*' }));
add('qty-and-eq', filterOp(2, { criteria1: '>=3', operator: 1, criteria2: '<=5' }));
for (const [label, op, v] of [['top3', 3, '3'], ['bottom3', 4, '3'], ['top30pct', 5, '30'], ['bottom30pct', 6, '30'], ['top1', 3, '1'], ['top100pct', 5, '100'], ['top50pct', 5, '50'], ['top20', 3, '20'], ['bottom1pct', 6, '1']])
  add('qty-' + label, filterOp(2, { criteria1: v, operator: op }));
add('date-top3', filterOp(3, { criteria1: '3', operator: 3 }));
add('values-two', filterOp(1, { criteria1: ['apple', 'Banana'], operator: 7 }));
add('values-case', filterOp(1, { criteria1: ['APPLE'], operator: 7 }));
add('values-blank', filterOp(1, { criteria1: ['apple', ''], operator: 7 }));
add('values-blank-eq', filterOp(1, { criteria1: ['='], operator: 7 }));
add('values-numbers', filterOp(2, { criteria1: ['5', '7.5'], operator: 7 }));
add('values-numbers-fmt', { ...filterOp(2, { criteria1: ['5.00', '3.00'], operator: 7 }) }, { formats: [{ range: 'B2:B13', numfmt: '0.00' }] });
add('multi-column', [filterOp(1, { criteria1: '=a*' }), filterOp(2, { criteria1: '>4' })]);
add('multi-then-clear-first', [filterOp(1, { criteria1: '=a*' }), filterOp(2, { criteria1: '>4' }), { kind: 'filter', range: R, field: 1 }]);
add('reapply', [filterOp(2, { criteria1: '>4' }), { kind: 'set', cells: { B3: 9, B4: 1 } }, { kind: 'reapply' }]);
add('show-all', [filterOp(2, { criteria1: '>4' }), { kind: 'show-all' }]);
add('hidden-before', [{ kind: 'hide-rows', range: '3:3' }, filterOp(2, { criteria1: '>4' })]);
add('date-gt', filterOp(3, { criteria1: '>' + dateText(plus(0)) }));
add('date-eq', filterOp(3, { criteria1: '=' + dateText(plus(1)) }));
add('date-between', filterOp(3, { criteria1: '>=' + dateText(plus(-10)), operator: 1, criteria2: '<=' + dateText(plus(10)) }));
const dyn = { today: 1, yesterday: 2, tomorrow: 3, thisweek: 4, lastweek: 5, nextweek: 6, thismonth: 7, lastmonth: 8, nextmonth: 9, thisquarter: 10, lastquarter: 11, nextquarter: 12, thisyear: 13, lastyear: 14, nextyear: 15, ytd: 16, q1: 17, q2: 18, q3: 19, q4: 20, m1: 21, m2: 22, m3: 23, m4: 24, m5: 25, m6: 26, m7: 27, m8: 28, m9: 29, m10: 30, m11: 31, m12: 32, above: 33, below: 34 };
for (const [k, v] of Object.entries(dyn)) add('dyn-' + k, filterOp(k === 'above' || k === 'below' ? 2 : 3, { criteria1: v, operator: 11 }));
// Date tree: pairs of (grouping, date).
add('tree-year', filterOp(3, { operator: 7, criteria1: [], criteria2: [0, dateText(plus(0))] }));
add('tree-month', filterOp(3, { operator: 7, criteria1: [], criteria2: [1, dateText(plus(0))] }));
add('tree-day', filterOp(3, { operator: 7, criteria1: [], criteria2: [2, dateText(plus(0))] }));
add('tree-two-days', filterOp(3, { operator: 7, criteria1: [], criteria2: [2, dateText(plus(0)), 2, dateText(plus(1))] }));
add('tree-year-month-mixed', filterOp(3, { operator: 7, criteria1: [], criteria2: [0, dateText(plus(-400)), 1, dateText(plus(1))] }));
add('tree-values-too', filterOp(3, { operator: 7, criteria1: [dateText(plus(-2))], criteria2: [0, dateText(plus(100))] }));
// Colours.
const colourFormats = [{ range: 'A2', fill: 'FF0000' }, { range: 'A4', fill: '00FF00' }, { range: 'A6', fill: 'FF0000' }, { range: 'A3', font: '0000FF' }, { range: 'A5', font: 'FF0000' }];
add('colour-cell-red', filterOp(1, { criteria1: 'FF0000', operator: 8, color: true }), { formats: colourFormats });
add('colour-font-blue', filterOp(1, { criteria1: '0000FF', operator: 9, color: true }), { formats: colourFormats });
add('colour-nofill', filterOp(1, { operator: 12 }), { formats: colourFormats });
add('colour-font-auto', filterOp(1, { operator: 13 }), { formats: colourFormats });
const iconFormats = [{ range: 'B2:B13', icons: 1 }];
add('icon-2', filterOp(2, { operator: 10, iconSet: 1, iconId: 2 }), { formats: iconFormats });
add('icon-none', filterOp(2, { operator: 14 }), { formats: iconFormats });
for (const pct of [15, 25, 35, 45, 55, 75, 95]) add('qty-top' + pct + 'pct', filterOp(2, { criteria1: String(pct), operator: 5 }));
for (const pct of [15, 25, 45, 75]) add('qty-bottom' + pct + 'pct', filterOp(2, { criteria1: String(pct), operator: 6 }));
add('date-bottom2', filterOp(3, { criteria1: '2', operator: 4 }));
add('qty-top2-with-text-col-filter', [filterOp(1, { criteria1: '<>apple' }), filterOp(2, { criteria1: '2', operator: 3 })]);
add('manual-hidden-then-filter', [{ kind: 'hide-rows', range: '2:2' }, filterOp(2, { criteria1: '>4' })]);
add('filter-then-show-all-then-other', [filterOp(2, { criteria1: '>4' }), { kind: 'show-all' }, filterOp(1, { criteria1: '=a*' })]);
add('clear-one-column', [filterOp(1, { criteria1: '=a*' }), filterOp(2, { criteria1: '>4' }), { kind: 'filter', range: R, field: 2 }]);
add('replace-criteria-same-column', [filterOp(2, { criteria1: '>4' }), filterOp(2, { criteria1: '<4' })]);
add('and-text-numbers', filterOp(2, { criteria1: '>=3', operator: 1, criteria2: '<>5' }));
add('or-wildcards', filterOp(1, { criteria1: '=a*', operator: 2, criteria2: '=*e' }));
add('or-equals-blank', filterOp(1, { criteria1: '=apple', operator: 2, criteria2: '=' }));
add('date-top-percent', filterOp(3, { criteria1: '25', operator: 5 }));
add('date-lt', filterOp(3, { criteria1: '<' + dateText(plus(0)) }));
add('date-ne', filterOp(3, { criteria1: '<>' + dateText(plus(0)) }));
add('date-eq-serial', filterOp(3, { criteria1: String(serialOf(plus(1))) }));
add('date-year-gt', filterOp(3, { criteria1: '>1/1/2027' }));
add('note-column-blank-nonblank', filterOp(4, { criteria1: '<>' }));
add('note-column-blank', filterOp(4, { criteria1: '=' }));
add('note-contains-x-or-y', filterOp(4, { criteria1: 'x', operator: 2, criteria2: 'y' }));
fs.writeFileSync(new URL('./filter.json', import.meta.url), JSON.stringify(out.map(s => ({ ...s, setup: [{ kind: 'filter-on', range: R }] }))));
console.log(out.length, serialOf(today));

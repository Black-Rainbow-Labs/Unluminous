import React, { useEffect, useRef, useState } from 'react';
import { Led, Screen, textWidth } from './Instrument.jsx';

/** Whether a cell reads as a number, which decides its face (Rust `table::is_figure`). */
export function isFigure(cell) {
  const trimmed = cell.trim().replace(/^[$£€+\-−]+/, '');
  if (!/^[0-9]/.test(trimmed)) return false;
  const digits = (trimmed.match(/[0-9]/g) || []).length;
  const visible = (trimmed.match(/\S/g) || []).length;
  return digits * 2 >= visible;
}

const PAD = 12;

function measureCell(cell) {
  return isFigure(cell) ? textWidth(cell, { size: 11.5, family: 'mono' }) : textWidth(cell, { size: 12 });
}

/**
 * How wide each column is. A short column keeps its natural width and a long one gives way: every column
 * first gets what it needs or a fair share, whichever is less; what is left goes to the columns that
 * wanted more, in proportion to what they wanted. Rust `Table::widths`.
 */
export function columnWidths(columns, rows, width) {
  const wants = columns.map((column, at) => {
    const head = textWidth(column.label.toUpperCase(), { size: 9.5, weight: 600, tracking: 0.12 });
    const cells = Math.max(0, ...rows.map((row) => (row[at] === undefined ? 0 : measureCell(row[at]))));
    return Math.max(head, cells) + PAD;
  });
  const room = Math.max(width - PAD, 1);
  const total = wants.reduce((a, b) => a + b, 0);
  if (!wants.length) return wants;
  if (total <= room) {
    const out = wants.slice();
    out[0] += room - total;
    return out;
  }
  const out = new Array(wants.length).fill(0);
  let open = wants.map((_, i) => i);
  let left = room;
  for (;;) {
    const share = left / Math.max(open.length, 1);
    const fits = open.filter((at) => wants[at] <= share);
    const rest = open.filter((at) => wants[at] > share);
    if (!fits.length) {
      const wanted = rest.reduce((sum, at) => sum + wants[at], 0);
      rest.forEach((at) => (out[at] = (left * wants[at]) / Math.max(wanted, 1)));
      break;
    }
    fits.forEach((at) => {
      out[at] = wants[at];
      left -= wants[at];
    });
    open = rest;
    if (!open.length) break;
  }
  return out.map((w) => Math.max(w, 40));
}

/**
 * A table on a screen: silkscreen headers, hairline rows, figures in tabular mono and right aligned,
 * words wrapped to at most three lines. Rust `table::Table`.
 *
 * `columns` is `[{ label, align: 'left' | 'right' | 'centre' }]`, `rows` is string[][]. The order is the
 * caller's: `sorted` is `[columnIndex, descending]` and `onSort(columnIndex)` reports a pressed header.
 */
export function Table({ columns = [], rows = [], sorted, onSort, className = '', style, ...rest }) {
  const ref = useRef(null);
  const [width, setWidth] = useState(480);
  useEffect(() => {
    if (!ref.current || typeof ResizeObserver === 'undefined') return undefined;
    const watch = new ResizeObserver(([entry]) => setWidth(entry.contentRect.width));
    watch.observe(ref.current);
    setWidth(ref.current.getBoundingClientRect().width);
    return () => watch.disconnect();
  }, []);

  const widths = columnWidths(columns, rows, width);
  const template = widths.map((w) => `${w}px`).join(' ');
  const alignOf = (column, figure, at) => (column.align === 'right' || (column.align !== 'centre' && figure && at > 0) ? 'right' : column.align === 'centre' ? 'centre' : 'left');

  return (
    <Screen radius={9} className={`rux-table ${className}`.trim()} style={style} {...rest}>
      <div ref={ref} role="table" className="rux-table__inner">
        <div role="row" className="rux-table__head" style={{ gridTemplateColumns: template }}>
          {columns.map((column, at) => {
            const sort = sorted && sorted[0] === at ? sorted : null;
            return (
              <button
                key={at}
                type="button"
                role="columnheader"
                aria-label={`Sort by ${column.label}`}
                className={`rux-table__th is-${column.align || 'left'}${sort ? ' is-sorted' : ''}`}
                onClick={() => onSort && onSort(at)}
              >
                {sort ? <Led colour="var(--accent-blue)" radius={2} className="rux-table__sort-led" /> : null}
                <span className="rux-silk">{column.label + (sort ? (sort[1] ? ' ▾' : ' ▴') : '')}</span>
              </button>
            );
          })}
        </div>
        {rows.map((cells, index) => (
          <div key={index} role="row" className="rux-table__row" style={{ gridTemplateColumns: template }}>
            {columns.map((column, at) => {
              const cell = cells[at];
              if (cell === undefined) return <span key={at} />;
              const figure = isFigure(cell);
              const cls = ['rux-table__cell', figure ? 'is-figure' : 'is-words', at === 0 && 'is-first', `is-${alignOf(column, figure, at)}`].filter(Boolean).join(' ');
              return (
                <span key={at} role="cell" className={cls}>
                  {cell}
                </span>
              );
            })}
          </div>
        ))}
        <div className="rux-table__foot" />
      </div>
    </Screen>
  );
}

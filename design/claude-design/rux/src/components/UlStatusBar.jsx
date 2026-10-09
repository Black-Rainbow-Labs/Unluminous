import React from 'react';

const cx = (...parts) => parts.filter(Boolean).join(' ');

/**
 * The bar along the very bottom of the window (components::status_bar): the file, whether it is saved,
 * its kind and the caret on the left, a message after them, and the font on the right.
 * @param items - the left hand items, each a string or { text, unsaved }
 * @param message - a sentence after the items, such as `task-2 moved to AGENT DONE`
 * @param right - the right hand item
 */
export function UlStatusBar({ items = ['program.rs', 'Rust', 'LF', 'Ln 1, Col 1'], message, right = 'Consolas · 14 pt', className, style, ...rest }) {
  return (
    <footer className={cx('ul-status', className)} style={style} {...rest}>
      {items.map((item, index) => {
        const entry = typeof item === 'string' ? { text: item } : item;
        return (
          <span key={index} className={cx('ul-status__item', index === 0 && 'is-file')}>
            {entry.unsaved && <span className="ul-status__dot" />}
            {entry.text}
          </span>
        );
      })}
      {message && <span className="ul-status__message">{message}</span>}
      <span className="ul-status__spacer" />
      <span className="ul-status__right">{right}</span>
    </footer>
  );
}

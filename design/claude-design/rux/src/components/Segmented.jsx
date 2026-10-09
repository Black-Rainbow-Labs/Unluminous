import React, { useState } from 'react';
import { Icon } from '../icon.jsx';

const cx = (...parts) => parts.filter(Boolean).join(' ');

/**
 * One row of mutually exclusive choices inside a pressed track (Rust `Segmented`).
 * items: [{ label, icon?, accent? }]. `accent` is a CSS colour for the chosen item's ink.
 * Controlled with `value` (index) and `onChange(index)`; falls back to internal state.
 */
export function Segmented({ items, value, defaultValue = 0, onChange, compact = false, className, style, ...rest }) {
  const [inner, setInner] = useState(defaultValue);
  const active = value ?? inner;
  const mark = compact ? 11 : 13;
  const choose = (index) => {
    if (value === undefined) setInner(index);
    if (onChange) onChange(index);
  };
  return (
    <div role="radiogroup" className={cx('rux-segmented', compact && 'rux-segmented--compact', className)} style={style} {...rest}>
      {items.map((item, index) => {
        const on = index === active;
        return (
          <button
            key={item.label}
            type="button"
            role="radio"
            aria-checked={on}
            className={cx('rux-segmented__item', on && 'is-active')}
            style={on && item.accent ? { color: item.accent } : undefined}
            onClick={() => choose(index)}
          >
            {item.icon && <Icon name={item.icon} size={mark} />}
            <span>{item.label}</span>
          </button>
        );
      })}
    </div>
  );
}

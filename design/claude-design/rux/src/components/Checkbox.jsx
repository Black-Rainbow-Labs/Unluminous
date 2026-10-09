import React, { useState } from 'react';

/**
 * A checkbox: a 20px recessed well with a tick in it, with its label beside it. Rust `key::Checkbox`.
 * `settles` quiets a ticked item's words (a done item on a checklist); a form's toggle passes false.
 */
export function Checkbox({
  checked,
  defaultChecked = false,
  label,
  colour,
  settles = true,
  onChange,
  className = '',
  style,
  ...rest
}) {
  const [inner, setInner] = useState(defaultChecked);
  const on = checked ?? inner;
  const toggle = () => {
    if (checked === undefined) setInner(!on);
    if (onChange) onChange(!on);
  };
  const cls = ['rux-checkbox', on && 'is-on', on && settles && 'is-settled', className].filter(Boolean).join(' ');
  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={on}
      className={cls}
      onClick={toggle}
      style={colour ? { '--rux-check-colour': colour, ...style } : style}
      {...rest}
    >
      <span className="rux-checkbox__cap">
        <svg className="rux-checkbox__tick" viewBox="0 0 20 20" aria-hidden="true">
          <polyline points="5.8,10.4 8.8,13.4 14.4,7.2" fill="none" strokeWidth="1.9" />
        </svg>
      </span>
      <span className="rux-checkbox__label">{label}</span>
    </button>
  );
}

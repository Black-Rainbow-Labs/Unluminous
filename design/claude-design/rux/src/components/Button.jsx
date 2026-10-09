import React from 'react';
import { Icon } from '../icon.jsx';

const cx = (...parts) => parts.filter(Boolean).join(' ');
const MARK = { mini: 12, sm: 12, md: 13, lg: 15 };

/** A button with a word on it. Seven variants, four sizes (Rust `Button`). */
export function Button({ label, children, variant = 'secondary', size = 'sm', icon, trailing, disabled = false, stretch = false, className, style, type = 'button', ...rest }) {
  const mark = MARK[size] || 12;
  return (
    <button
      type={type}
      disabled={disabled}
      className={cx('rux-button', `rux-button--${variant}`, `rux-button--${size}`, stretch && 'rux-button--stretch', disabled && 'is-disabled', className)}
      style={style}
      {...rest}
    >
      {icon && <Icon name={icon} size={mark} />}
      <span className="rux-button__label">{label ?? children}</span>
      {trailing && <Icon name={trailing} size={mark} />}
    </button>
  );
}

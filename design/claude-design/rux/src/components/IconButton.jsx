import React from 'react';
import { Icon } from '../icon.jsx';

const cx = (...parts) => parts.filter(Boolean).join(' ');
const MARK = { tiny: 12, sm: 13, md: 14, lg: 17, xl: 19 };

/** A round button with a mark in it (Rust `IconButton`). `label` is the accessible name. */
export function IconButton({ icon, label, size = 'lg', variant = 'raised', dot, badge, tint, turn = 0, disabled = false, className, style, type = 'button', ...rest }) {
  const vars = { ...(tint ? { color: tint } : null), ...(dot ? { '--rux-dot': dot } : null), ...style };
  return (
    <button
      type={type}
      aria-label={label}
      disabled={disabled}
      className={cx('rux-icon-button', `rux-icon-button--${variant}`, `rux-icon-button--${size}`, tint && 'has-tint', disabled && 'is-disabled', className)}
      style={vars}
      {...rest}
    >
      <span className="rux-icon-button__mark" style={turn ? { transform: `rotate(${turn}rad)` } : undefined}>
        <Icon name={icon} size={MARK[size] || 17} />
      </span>
      {dot && <span className="rux-icon-button__dot" aria-hidden="true" />}
      {badge != null && <span className="rux-icon-button__badge">{badge}</span>}
    </button>
  );
}

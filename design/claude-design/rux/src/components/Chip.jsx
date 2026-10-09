import React from 'react';
import { Icon } from '../icon.jsx';

const cx = (...parts) => parts.filter(Boolean).join(' ');

/** A small pill that says something rather than does something (Rust `Chip`). `accent` picks the gradient of an accent chip. */
export function Chip({ label, children, value, dot, icon, dismissible = false, onDismiss, variant = 'sunken', mono = false, accent = 'mint', className, style, ...rest }) {
  const vars = { ...(dot ? { '--rux-chip-dot': dot } : null), ...style };
  return (
    <span
      className={cx('rux-chip', `rux-chip--${variant}`, variant === 'accent' && `rux-chip--${accent}`, mono && 'rux-chip--mono', className)}
      style={vars}
      {...rest}
    >
      {dot && <span className="rux-chip__dot" aria-hidden="true" />}
      {icon && <Icon name={icon} size={12} />}
      <span className="rux-chip__text">
        {label ?? children}
        {value != null && <b className="rux-chip__value">{value}</b>}
      </span>
      {dismissible && (
        <button type="button" className="rux-chip__x" aria-label="Dismiss" onClick={onDismiss}>
          <Icon name="x" size={9} stroke={2.4} />
        </button>
      )}
    </span>
  );
}

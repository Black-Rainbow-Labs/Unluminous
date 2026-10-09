import React from 'react';

const cx = (...parts) => parts.filter(Boolean).join(' ');

/** A round chip standing in for a person (Rust `Avatar`, `.avatar`). `status` is a CSS colour for the dot; `diameter` is in px. */
export function Avatar({ initials, status, diameter = 40, name = 'Account', className, style, type = 'button', ...rest }) {
  const vars = { '--d': `${diameter}px`, '--k': diameter / 40, ...(status ? { '--rux-status': status } : null), ...style };
  return (
    <button type={type} aria-label={name} title={name} className={cx('rux-avatar', className)} style={vars} {...rest}>
      <span className="rux-avatar__initials">{initials}</span>
      {status && <span className="rux-avatar__status" aria-hidden="true" />}
    </button>
  );
}

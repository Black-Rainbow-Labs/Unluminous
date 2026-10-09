import React from 'react';

const cx = (...parts) => parts.filter(Boolean).join(' ');

/** A bar that fills as something finishes (Rust `Progress`). `fraction` is 0 to 1; omit it for an indeterminate sweep. `height` is in px. */
export function Progress({ fraction, height = 6, label = 'Progress', className, style, ...rest }) {
  const known = typeof fraction === 'number';
  const clamped = known ? Math.min(1, Math.max(0, fraction)) : 0;
  return (
    <div
      role="progressbar"
      aria-label={label}
      aria-valuemin={known ? 0 : undefined}
      aria-valuemax={known ? 100 : undefined}
      aria-valuenow={known ? Math.round(clamped * 100) : undefined}
      className={cx('rux-progress', !known && 'rux-progress--indeterminate', className)}
      style={{ height, ...style }}
      {...rest}
    >
      <span className="rux-progress__fill" style={known ? { width: `${clamped * 100}%` } : undefined} />
    </div>
  );
}

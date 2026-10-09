import React from 'react';

const cx = (...parts) => parts.filter(Boolean).join(' ');

/** A sunken block standing in for something that has not arrived, with a highlight sweeping over it (Rust `Skeleton`). Size it with `style` or a class. `radius` is a CSS length. */
export function Skeleton({ radius, className, style, ...rest }) {
  return (
    <span
      aria-hidden="true"
      className={cx('rux-skeleton', className)}
      style={{ ...(radius != null ? { borderRadius: radius } : null), ...style }}
      {...rest}
    >
      <span className="rux-skeleton__sweep" />
    </span>
  );
}

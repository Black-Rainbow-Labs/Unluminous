import React from 'react';

const cx = (...parts) => parts.filter(Boolean).join(' ');

/**
 * A scrolling area with the rux scrollbar (Rust `Scrollbar`): a 6px thumb in `--surface-sunken`, no track,
 * 6px in from the edge, `--ink-300` under the pointer. Give it a height or max height through `style`.
 */
export function Scrollbar({ children, className, style, ...rest }) {
  return (
    <div className={cx('rux-scrollbar', className)} style={style} {...rest}>
      {children}
    </div>
  );
}

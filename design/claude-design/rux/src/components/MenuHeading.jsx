import React from 'react';

/**
 * MenuHeading: the small mono heading over a group of menu rows (Rust `menu_heading`, `.proj__menu-head`).
 * Fixed 28px tall: padding 10px 14px 8px, mono 9px uppercase at 0.14em, `--ink-400`.
 */
export function MenuHeading({ children, className = '', style, ...rest }) {
  return (
    <div className={('rux-menu-heading ' + className).trim()} style={style} {...rest}>{children}</div>
  );
}

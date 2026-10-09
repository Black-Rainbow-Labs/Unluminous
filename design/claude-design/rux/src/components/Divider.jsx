import React from 'react';

/**
 * Divider: a 1px rule (Rust `Divider`). `faded` fades out at both ends (`.topbar::after`).
 * `color` is a CSS colour or var(); the default is `--hairline`.
 */
export function Divider({ faded = false, color, className = '', style, ...rest }) {
  const vars = color ? { '--divider-color': color } : {};
  return (
    <div
      role="separator"
      className={('rux-divider ' + (faded ? 'rux-divider--faded ' : '') + className).trim()}
      style={{ ...vars, ...style }}
      {...rest}
    />
  );
}

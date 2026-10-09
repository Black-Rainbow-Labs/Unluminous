import React from 'react';

/**
 * Panel: a card that stands off the page, or a flat panel (Rust `Panel`, `PanelKind`).
 * `kind`: "raised" (`--e-raised`), "raisedSmall" (`--e-raised-sm`), "flat" (no shadow).
 * `radius` is a CSS length (default `--r-xl`), `pad` a CSS padding (default 22px), `fill` a CSS colour or var().
 */
export function Panel({ kind = 'raised', radius, pad, fill, children, className = '', style, ...rest }) {
  const vars = {};
  if (radius !== undefined) vars['--panel-radius'] = typeof radius === 'number' ? radius + 'px' : radius;
  if (pad !== undefined) vars['--panel-pad'] = typeof pad === 'number' ? pad + 'px' : pad;
  if (fill !== undefined) vars['--panel-fill'] = fill;
  const kebab = kind === 'raisedSmall' ? 'raised-small' : kind;
  return (
    <div className={('rux-panel rux-panel--' + kebab + ' ' + className).trim()} style={{ ...vars, ...style }} {...rest}>
      {children}
    </div>
  );
}

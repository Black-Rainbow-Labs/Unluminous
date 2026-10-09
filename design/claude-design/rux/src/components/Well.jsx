import React from 'react';

/**
 * Well: a surface pressed into the page (Rust `Well`; `.timeline`, `.ic-prompt-wrap`, `.ic-tile`).
 * `shallow` uses `--e-pressed-sm` instead of `--e-pressed`. `radius`, `pad` and `fill` as for Panel.
 */
export function Well({ shallow = false, radius, pad, fill, children, className = '', style, ...rest }) {
  const vars = {};
  if (radius !== undefined) vars['--well-radius'] = typeof radius === 'number' ? radius + 'px' : radius;
  if (pad !== undefined) vars['--well-pad'] = typeof pad === 'number' ? pad + 'px' : pad;
  if (fill !== undefined) vars['--well-fill'] = fill;
  return (
    <div className={('rux-well ' + (shallow ? 'rux-well--shallow ' : '') + className).trim()} style={{ ...vars, ...style }} {...rest}>
      {children}
    </div>
  );
}

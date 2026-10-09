import React from 'react';
import icons from '../ul-icons.data.json';

/** Every Unluminous mark name, in the material set. */
export const UL_ICON_NAMES = Object.keys(icons.material);

/**
 * One of Unluminous's own drawn marks (theme::icon), in the current text colour.
 * The drawing is the shapes the Rust painter call makes, exported on a 24 point cell with the mark
 * centred on 12,12; `size` scales the whole cell.
 * @param name - the mark, e.g. "folder", "terminal", "debug-run"
 * @param set - "material" (what a window opens in) or "classic"
 * @param size - the side of the cell in px
 */
export function UlIcon({ name, set = 'material', size = 24, className = '', style, ...rest }) {
  const inner = (set === 'classic' && icons.classic[name]) || icons.material[name];
  if (!inner) return null;
  return (
    <svg
      className={`ul-icon ${className}`.trim()}
      style={style}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      aria-hidden={rest['aria-label'] ? undefined : true}
      focusable="false"
      dangerouslySetInnerHTML={{ __html: inner }}
      {...rest}
    />
  );
}

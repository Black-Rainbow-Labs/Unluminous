import React from 'react';
import { Icon } from '../icon.jsx';

const cx = (...parts) => parts.filter(Boolean).join(' ');

/**
 * A 16:9 box where a picture goes (Rust `Tile`). kind `well` is a pressed well (`.ic-tile`);
 * kind `dropzone` is a dark clickable dropzone (`.scene__image`) that shows `title` and `subtitle` while empty.
 * Children are drawn over the box, inside 12px of padding.
 */
export function Tile({ kind = 'well', image, title, subtitle, label = 'Tile', children, className, style, onClick, ...rest }) {
  const dropzone = kind === 'dropzone';
  const Root = dropzone ? 'button' : 'div';
  const props = dropzone ? { type: 'button', onClick } : { onClick };
  return (
    <Root aria-label={label} className={cx('rux-tile', `rux-tile--${kind}`, className)} style={style} {...props} {...rest}>
      {image && <img className="rux-tile__image" src={image} alt="" />}
      {!image && dropzone && title && (
        <span className="rux-tile__empty">
          <span className="rux-tile__empty-icon"><Icon name="imagePlus" size={22} /></span>
          <span className="rux-tile__title">{title}</span>
          {subtitle && <span className="rux-tile__sub">{subtitle}</span>}
        </span>
      )}
      {children && <span className="rux-tile__content">{children}</span>}
    </Root>
  );
}

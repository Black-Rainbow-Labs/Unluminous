import React from 'react';
import { Icon } from '../icon.jsx';

/**
 * TopNav: the pill of navigation items along the top of the page (Rust `TopNav`, `NavItem`; `.topnav`).
 * `items` is a list of `{ id, icon, label }` (the Rust `NavItem`); `active` is an index.
 * Only the active item shows its label. `fit` is the Rust `Fit`: "full", "icons" (marks only) or
 * "trimmed" (marks only with 10px side padding), for when the strip has less room than it wants.
 */
export function TopNav({ items = [], active = 0, onChange, fit = 'full', className = '', style, ...rest }) {
  return (
    <nav className={('rux-top-nav rux-top-nav--' + fit + ' ' + className).trim()} style={style} aria-label="Navigation" {...rest}>
      {items.map((item, index) => {
        const isActive = index === active;
        return (
          <button
            type="button" key={item.id || index} aria-label={item.label} aria-current={isActive ? 'page' : undefined}
            className={'rux-top-nav__item' + (isActive ? ' is-active' : '')}
            onClick={() => { if (onChange) onChange(index); }}
          >
            <Icon name={item.icon} size={20} />
            <span className="rux-top-nav__label">{item.label}</span>
          </button>
        );
      })}
    </nav>
  );
}

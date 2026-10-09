import React from 'react';
import { Icon } from '../icon.jsx';

/**
 * ListItem: one row in a list or menu (Rust `ListItem`; `.proj__item` and `.ic-modal__list-item`).
 * `selected` takes the accent, or is pressed into the surface with `pressedWhenSelected`.
 * `action={{ icon, label }}` adds a trailing button that shows on hover or when selected.
 */
export function ListItem({
  label, selected = false, dot = false, tick = false, action = null,
  pressedWhenSelected = false, onClick, onAction, className = '', style, ...rest
}) {
  const classes = [
    'rux-list-item', selected && 'is-selected', pressedWhenSelected && 'rux-list-item--pressed',
    className,
  ].filter(Boolean).join(' ');
  const trailing = tick || action;
  return (
    <div
      role="option" aria-selected={selected} tabIndex={0} className={classes} style={style}
      onClick={onClick}
      onKeyDown={(event) => { if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); if (onClick) onClick(event); } }}
      {...rest}
    >
      {dot && <span className="rux-list-item__dot" aria-hidden="true" />}
      <span className="rux-list-item__label">{label}</span>
      {trailing && (
        <span className="rux-list-item__trail">
          {action ? (
            <button
              type="button" className="rux-list-item__action" aria-label={action.label}
              onClick={(event) => { event.stopPropagation(); if (onAction) onAction(event); }}
            >
              <Icon name={action.icon} size={12} stroke={2.4} />
            </button>
          ) : (
            selected && <Icon name="check" size={14} />
          )}
        </span>
      )}
    </div>
  );
}

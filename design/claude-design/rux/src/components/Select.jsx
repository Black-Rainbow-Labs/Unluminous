import React from 'react';
import { Icon } from '../icon.jsx';

/**
 * Select: a trigger that says what is chosen and a menu that opens under it (Rust `Select`, `.ic-select`).
 * `options` is a list of strings; `value` is the chosen index or null. `open` is controlled when passed.
 * `zoom` scales words, padding, chevron and menu together. `menuElevation="quiet"` uses `--e-raised-sm`.
 */
export function Select({
  options = [], value = null, onChange, placeholder = '—', mono = false, up = false,
  disabled = false, label = 'Select', zoom = 1, menuElevation = 'default',
  open: openProp, onToggle, className = '', style, ...rest
}) {
  const [openState, setOpenState] = React.useState(false);
  const controlled = openProp !== undefined;
  const open = controlled ? openProp : openState;
  const root = React.useRef(null);
  const setOpen = (next) => {
    if (!controlled) setOpenState(next);
    if (onToggle) onToggle(next);
  };

  React.useEffect(() => {
    if (!open) return undefined;
    const away = (event) => { if (root.current && !root.current.contains(event.target)) setOpen(false); };
    const key = (event) => { if (event.key === 'Escape') setOpen(false); };
    document.addEventListener('mousedown', away);
    document.addEventListener('keydown', key);
    return () => { document.removeEventListener('mousedown', away); document.removeEventListener('keydown', key); };
  });

  const chosen = value != null ? options[value] : undefined;
  const classes = [
    'rux-select', mono && 'rux-select--mono', up && 'rux-select--up',
    menuElevation === 'quiet' && 'rux-select--quiet', open && 'is-open', disabled && 'is-disabled', className,
  ].filter(Boolean).join(' ');

  return (
    <div ref={root} className={classes} style={{ '--z': zoom, ...style }} {...rest}>
      <button
        type="button" className="rux-select__trigger" disabled={disabled}
        aria-haspopup="listbox" aria-expanded={open} aria-label={label}
        onClick={() => setOpen(!open)}
      >
        <span className={'rux-select__value' + (chosen === undefined ? ' rux-select__value--empty' : '')}>
          {chosen === undefined ? placeholder : chosen}
        </span>
        <span className="rux-select__chev"><Icon name="chevDown" size={13 * zoom} stroke={2 * zoom} /></span>
      </button>
      {open && (
        <div className="rux-select__menu" role="listbox" aria-label={label}>
          {options.map((option, index) => (
            <button
              type="button" key={index} role="option" aria-selected={value === index}
              className={'rux-select__item' + (value === index ? ' is-selected' : '')}
              onClick={() => { if (onChange) onChange(index); setOpen(false); }}
            >
              <span className="rux-select__item-label">{option}</span>
              {value === index && <Icon name="check" size={12 * zoom} />}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

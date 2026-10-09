import React from 'react';
import { Icon } from '../icon.jsx';

/**
 * SubGroup: a collapsible group, a heading with a disclosure and a hairline over it (Rust `SubGroup`, `.ic-sub`).
 * `accent` is one of blue, violet, amber, rose, mint, coral and tints the heading mark.
 * `first` removes the hairline and the 14px above (`.ic-sub:first-of-type`). `open` is controlled when passed.
 */
export function SubGroup({
  title, icon = 'layers', accent = 'blue', open: openProp, defaultOpen = true, first = false,
  onToggle, children, className = '', style, ...rest
}) {
  const [inner, setInner] = React.useState(defaultOpen);
  const controlled = openProp !== undefined;
  const open = controlled ? openProp : inner;
  const classes = ['rux-sub', first && 'rux-sub--first', !open && 'is-collapsed', className].filter(Boolean).join(' ');
  return (
    <section className={classes} style={{ '--sub-accent': 'var(--accent-' + accent + ')', ...style }} {...rest}>
      <button
        type="button" className="rux-sub__head" aria-expanded={open}
        onClick={() => { if (!controlled) setInner(!open); if (onToggle) onToggle(!open); }}
      >
        <span className="rux-sub__mark"><Icon name={icon} size={13} /></span>
        <span className="rux-sub__title">{title}</span>
        <span className="rux-sub__chev"><Icon name="chevDown" size={11} stroke={2.2} /></span>
      </button>
      {open && <div className="rux-sub__body">{children}</div>}
    </section>
  );
}

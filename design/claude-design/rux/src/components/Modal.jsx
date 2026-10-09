import React from 'react';
import { Icon } from '../icon.jsx';

/**
 * Modal: a dialog over a dimmed page (Rust `Modal`; `.ic-modal-backdrop`, `.ic-modal`).
 * `footer` is a node (a footer strip is drawn when given). `left` + `leftWidth` split the body in two, as
 * Rust `split_body` does (`grid-template-columns: 260px 1fr` with a hairline between).
 * `present`: "dialog" (centred) or "sheet" (up from the bottom edge, top corners rounded).
 * `inline` draws the scrim and dialog inside a box of `boxHeight` px instead of covering the page.
 * `raised` uses `.ic-modal`'s own `--e-raised-lg` plus a 30px 80px drop, instead of `--e-modal`.
 */
export function Modal({
  title, icon, iconTint, width = 920, height = 640, footer, left, leftWidth = 260, children,
  open = true, onClose, lightDismiss = true, raised = false, present = 'dialog', inline = false, boxHeight = 520,
  className = '', style, ...rest
}) {
  React.useEffect(() => {
    if (!open || inline) return undefined;
    const key = (event) => { if (event.key === 'Escape' && onClose) onClose(); };
    document.addEventListener('keydown', key);
    return () => document.removeEventListener('keydown', key);
  }, [open, inline, onClose]);

  if (!open) return null;
  const classes = [
    'rux-modal', 'rux-modal--' + present, inline && 'rux-modal--inline', raised && 'rux-modal--raised', className,
  ].filter(Boolean).join(' ');
  const vars = { '--modal-w': width + 'px', '--modal-h': height + 'px', '--modal-box-h': boxHeight + 'px', ...style };
  const tint = iconTint ? { color: iconTint } : undefined;

  return (
    <div className={classes} style={vars} {...rest}>
      <div className="rux-modal__scrim" onClick={() => { if (lightDismiss && onClose) onClose(); }} />
      <div className="rux-modal__frame" role="dialog" aria-modal="true" aria-label={title}>
        {present === 'sheet' && <div className="rux-modal__grab" aria-hidden="true" />}
        <header className="rux-modal__head">
          {icon && <span className="rux-modal__title-icon" style={tint}><Icon name={icon} size={14} /></span>}
          <h2 className="rux-modal__title">{title}</h2>
          <button type="button" className="rux-modal__close" aria-label="Close" onClick={onClose}>
            <Icon name="x" size={14} />
          </button>
        </header>
        <div className={'rux-modal__body' + (left ? ' rux-modal__body--split' : '')} style={left ? { gridTemplateColumns: leftWidth + 'px 1fr' } : undefined}>
          {left && <div className="rux-modal__left">{left}</div>}
          <div className="rux-modal__main">{children}</div>
        </div>
        {footer && <footer className="rux-modal__foot">{footer}</footer>}
      </div>
    </div>
  );
}

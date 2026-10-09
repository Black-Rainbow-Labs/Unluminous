import React from 'react';
import { UlIcon } from './UlIcon.jsx';
import { UlButton } from './UlButton.jsx';

const cx = (...parts) => parts.filter(Boolean).join(' ');

/**
 * A modal in Unluminous's own window (components::modal::show): explorer fill over a scrim, a one point
 * control_border stroke, corner radius 10, a 46 point header in title_bar with the title and a close
 * cross, and a 52 point footer with a divider along its top and the buttons at its right. The last
 * button is the one that does the thing: it is filled in the accent and Enter presses it.
 * @param title - the title at the left of the header
 * @param buttons - the footer's buttons, left to right; the last one is primary
 * @param note - the quiet sentence at the left of the footer
 * @param inline - draw over a scrim inside the parent box rather than over the whole page
 */
export function UlModal({ title = 'Settings', buttons = ['Cancel', 'Done'], note, width = 560, height = 360, inline = true, open = true, onClose, children, className, style, ...rest }) {
  if (!open) return null;
  return (
    <div className={cx('ul-modal-scrim', inline && 'is-inline')}>
      <section className={cx('ul-modal', className)} style={{ width, height, ...style }} role="dialog" aria-label={title} {...rest}>
        <header className="ul-modal__header">
          <span className="ul-modal__title">{title}</span>
          <button type="button" className="ul-modal__close" aria-label="Close" onClick={onClose}><UlIcon name="cross" size={20} /></button>
        </header>
        <div className="ul-modal__body">{children}</div>
        <footer className="ul-modal__footer">
          {note && <span className="ul-modal__note">{note}</span>}
          <span className="ul-modal__spacer" />
          {buttons.map((label, index) => (
            <UlButton key={label} label={label} variant={index === buttons.length - 1 ? 'primary' : 'default'} />
          ))}
        </footer>
      </section>
    </div>
  );
}

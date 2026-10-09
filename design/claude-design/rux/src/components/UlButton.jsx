import React from 'react';

const cx = (...parts) => parts.filter(Boolean).join(' ');

/**
 * A button with a word on it, the way Unluminous draws one in its own window (modal::button,
 * controls::choice_button).
 * @param variant - "default": control fill and a one point control_border stroke, label in
 *   text_strong; "primary": the accent fill a modal's last button takes, label in on_accent;
 *   "choice": a choice button, accent filled when `on`; "state": a state that is not a button, such as `In use`
 * @param on - for a choice button, whether what it stands for is on
 */
export function UlButton({ label, children, variant = 'default', on = false, disabled = false, className, style, type = 'button', ...rest }) {
  if (variant === 'state') {
    return <span className={cx('ul-button', 'ul-button--state', className)} style={style} {...rest}>{label ?? children}</span>;
  }
  return (
    <button
      type={type}
      disabled={disabled}
      aria-pressed={variant === 'choice' ? on : undefined}
      className={cx('ul-button', `ul-button--${variant}`, on && 'is-on', className)}
      style={style}
      {...rest}
    >
      {label ?? children}
    </button>
  );
}

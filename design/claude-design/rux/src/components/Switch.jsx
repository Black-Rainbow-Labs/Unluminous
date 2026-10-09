import React, { useState } from 'react';

const cx = (...parts) => parts.filter(Boolean).join(' ');

/** A switch: a pressed track with a raised knob that slides (Rust `Switch`, `.toggle__knob`). 44 by 24. */
export function Switch({ on, defaultOn = false, onChange, label = 'Toggle', disabled = false, className, style, ...rest }) {
  const [inner, setInner] = useState(defaultOn);
  const isOn = on ?? inner;
  const flip = () => {
    if (on === undefined) setInner(!isOn);
    if (onChange) onChange(!isOn);
  };
  return (
    <button
      type="button"
      role="switch"
      aria-checked={isOn}
      aria-label={label}
      disabled={disabled}
      className={cx('rux-switch', isOn && 'is-on', disabled && 'is-disabled', className)}
      style={style}
      onClick={flip}
      {...rest}
    >
      <span className="rux-switch__knob" />
    </button>
  );
}

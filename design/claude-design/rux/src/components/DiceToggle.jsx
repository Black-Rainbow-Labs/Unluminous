import React, { useState } from 'react';
import { Icon } from '../icon.jsx';

const cx = (...parts) => parts.filter(Boolean).join(' ');

/** A 28px round toggle beside a control, for "randomise this one" (Rust `DiceToggle`). The default mark is `cube`, as in the Rust. */
export function DiceToggle({ on, defaultOn = false, onChange, icon = 'cube', label = 'Randomize', disabled = false, className, style, ...rest }) {
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
      className={cx('rux-dice', isOn && 'is-on', disabled && 'is-disabled', className)}
      style={style}
      onClick={flip}
      {...rest}
    >
      <Icon name={icon} size={13} />
    </button>
  );
}

import React, { useState } from 'react';
import { Icon } from '../icon.jsx';

const cx = (...parts) => parts.filter(Boolean).join(' ');

/** A number with an up and a down tick beside it (Rust `Stepper`, `.ic-count`). Always the stacked desktop form. */
export function Stepper({ value, defaultValue = 0, min = 0, max = 100, onChange, label = 'Count', disabled = false, className, style, ...rest }) {
  const [inner, setInner] = useState(defaultValue);
  const current = value ?? inner;
  const set = (next) => {
    if (value === undefined) setInner(next);
    if (onChange) onChange(next);
  };
  const atMax = current >= max;
  const atMin = current <= min;
  return (
    <div className={cx('rux-stepper', disabled && 'is-disabled', className)} style={style} role="group" aria-label={label} {...rest}>
      <span className="rux-stepper__value" aria-live="polite">{current}</span>
      <span className="rux-stepper__ticks">
        <button type="button" className="rux-stepper__tick rux-stepper__tick--up" aria-label={`Increase ${label}`} disabled={disabled || atMax} onClick={() => set(Math.min(current + 1, max))}>
          <Icon name="chevDown" size={9} stroke={2.4} />
        </button>
        <button type="button" className="rux-stepper__tick" aria-label={`Decrease ${label}`} disabled={disabled || atMin} onClick={() => set(Math.max(current - 1, min))}>
          <Icon name="chevDown" size={9} stroke={2.4} />
        </button>
      </span>
    </div>
  );
}

import React from 'react';
import { Led } from './Instrument.jsx';

/**
 * A key cap: a raised plate with a lit top edge that sinks one point when pressed. Rust `key::Key`.
 *
 * `led` is `{ colour, lit }`. `chosen` is `{ colour, on }` for the chosen key in a row of choices or a
 * strip of tabs: held down, words in `colour`, no LED. `tinted` is a colour for the words alone.
 */
export function Key({
  label,
  children,
  led,
  down = false,
  chosen,
  tinted,
  disabled = false,
  compact = false,
  onClick,
  className = '',
  style,
  ...rest
}) {
  const isDown = down || Boolean(chosen && chosen.on);
  const lit = Boolean(led && led.lit) || isDown;
  const accent = chosen && chosen.on ? chosen.colour : tinted;
  const cls = [
    'rux-key',
    compact && 'rux-key--compact',
    isDown && 'is-down',
    lit && 'is-lit',
    disabled && 'is-disabled',
    className,
  ]
    .filter(Boolean)
    .join(' ');
  const words = label ?? children;
  return (
    <button
      type="button"
      className={cls}
      disabled={disabled}
      aria-pressed={chosen || down ? isDown : undefined}
      onClick={onClick}
      style={accent && !disabled ? { color: accent, ...style } : style}
      {...rest}
    >
      {led ? <Led colour={led.colour} brightness={lit ? 1 : 0} radius={compact ? 2.5 : 3} className="rux-key__led" /> : null}
      <span className="rux-key__label">{words}</span>
    </button>
  );
}

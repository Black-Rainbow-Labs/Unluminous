import React, { useRef, useState } from 'react';

/** `value` snapped to `step` and kept in `min..max` (Rust `Fader::snap`). */
export function snapFader(value, min, max, step) {
  const clamped = Math.min(max, Math.max(min, value));
  if (step > 0) {
    const steps = Math.round((clamped - min) / step);
    return Math.min(max, Math.max(min, min + steps * step));
  }
  return clamped;
}

/** How long the lit run is at `fraction` of a groove whose ticks run from `left` to `right` (Rust `Fader::lit_run`). */
export function litRun(fraction, left, right) {
  return Math.max(0, right - left) * Math.min(1, Math.max(0, fraction));
}

/**
 * A fader: a groove sunk into the plate, eleven ticks under it, the travelled part lit, and a raised cap
 * with two engraved lines. The groove is its scale: it runs from the cap's centre at `min` to its centre
 * at `max`. Rust `fader::Fader`. Dragged, pressed anywhere along the groove, or moved with the arrows.
 */
export function Fader({
  value,
  defaultValue = 0,
  min = 0,
  max = 1,
  step = 0,
  colour = 'var(--accent-blue)',
  label = 'Fader',
  onChange,
  className = '',
  style,
  ...rest
}) {
  const top = max > min ? max : min + 1;
  const [inner, setInner] = useState(defaultValue);
  const [dragging, setDragging] = useState(false);
  const ref = useRef(null);
  const current = value ?? inner;
  const fraction = Math.min(1, Math.max(0, (current - min) / (top - min)));

  const commit = (next) => {
    if (Math.abs(next - current) <= Number.EPSILON) return;
    if (value === undefined) setInner(next);
    if (onChange) onChange(next);
  };
  const valueAt = (clientX) => {
    const box = ref.current.getBoundingClientRect();
    const left = box.left + 6;
    const right = box.right - 6;
    const f = Math.min(1, Math.max(0, (clientX - left) / Math.max(1, right - left)));
    return snapFader(min + f * (top - min), min, top, step);
  };
  const onPointerDown = (event) => {
    ref.current.setPointerCapture(event.pointerId);
    ref.current.focus();
    setDragging(true);
    commit(valueAt(event.clientX));
  };
  const onPointerMove = (event) => {
    if (dragging) commit(valueAt(event.clientX));
  };
  const onKeyDown = (event) => {
    const unit = step > 0 ? step : (top - min) / 100;
    const more = event.key === 'ArrowRight' || event.key === 'ArrowUp';
    const less = event.key === 'ArrowLeft' || event.key === 'ArrowDown';
    if (!more && !less) return;
    event.preventDefault();
    commit(snapFader(current + (more ? unit : -unit), min, top, step));
  };

  const cls = ['rux-fader', dragging && 'is-dragging', className].filter(Boolean).join(' ');
  const travel = `calc((100% - 12px) * ${fraction})`;
  return (
    <div
      ref={ref}
      role="slider"
      tabIndex={0}
      aria-label={label}
      aria-valuemin={min}
      aria-valuemax={top}
      aria-valuenow={current}
      className={cls}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={() => setDragging(false)}
      onPointerCancel={() => setDragging(false)}
      onKeyDown={onKeyDown}
      style={{ '--rux-fader-colour': colour, ...style }}
      {...rest}
    >
      <span className="rux-fader__groove" />
      {fraction > 0 ? (
        <>
          <span className="rux-fader__glow" style={{ width: `calc(${travel} + 2px)` }} />
          <span className="rux-fader__lit" style={{ width: travel }} />
        </>
      ) : null}
      {Array.from({ length: 11 }, (_, tick) => (
        <span
          key={tick}
          className={`rux-fader__tick${tick % 5 === 0 ? ' is-tall' : ''}`}
          style={{ left: `calc(6px + (100% - 12px) * ${tick / 10})` }}
        />
      ))}
      <span className="rux-fader__cap rux-lit-edge" style={{ left: travel }}>
        <span className="rux-fader__grip" />
        <span className="rux-fader__grip" />
      </span>
    </div>
  );
}

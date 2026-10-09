import React, { useEffect, useRef, useState } from 'react';
import { alpha, easeOut, progress, TIMING, useClock } from './Instrument.jsx';

/**
 * A row of segments lit up to `fraction` on a screen, the way a level meter on a mixing desk is.
 * Rust `fader::Meter`. `top` is the colour of the last tenth. `animate` makes it rise from zero once.
 */
export function Meter({
  fraction = 0,
  colour = 'var(--accent-blue)',
  top,
  animate = false,
  label = 'Meter',
  className = '',
  style,
  ...rest
}) {
  const ref = useRef(null);
  const [width, setWidth] = useState(200);
  useEffect(() => {
    if (!ref.current || typeof ResizeObserver === 'undefined') return undefined;
    const watch = new ResizeObserver(([entry]) => setWidth(entry.contentRect.width));
    watch.observe(ref.current);
    setWidth(ref.current.getBoundingClientRect().width);
    return () => watch.disconnect();
  }, []);

  const f = Math.min(1, Math.max(0, fraction));
  const t = useClock(animate, TIMING.RISE * 1.5);
  const rise = animate ? easeOut(progress(t, 0, TIMING.RISE * 1.5)) : 1;
  const inside = Math.max(0, width - 14);
  const count = Math.max(4, Math.floor((inside + 2) / 8));
  const lit = Math.round(f * rise * count);
  return (
    <div
      ref={ref}
      role="meter"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={1}
      aria-valuenow={f}
      className={`rux-meter rux-screen ${className}`.trim()}
      style={{ '--rux-screen-r': '7px', ...style }}
      {...rest}
    >
      {Array.from({ length: count }, (_, segment) => {
        const on = segment < lit;
        const near = segment >= count * 0.9;
        const tint = on ? (near && top ? top : colour) : 'var(--inst-led-off)';
        return (
          <span
            key={segment}
            className="rux-meter__segment"
            style={{ background: tint, boxShadow: on ? `0 0 0 1.5px ${alpha(tint, 0.14)}` : undefined }}
          />
        );
      })}
    </div>
  );
}

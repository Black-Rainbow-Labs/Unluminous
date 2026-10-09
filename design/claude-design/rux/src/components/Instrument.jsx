import React, { useEffect, useState } from 'react';

/**
 * The instrument language (rux `components/instrument.rs`): plates, screens, LEDs, silkscreen
 * labels, readings, and the motion and measuring helpers the other instrument controls share.
 */

/** Seconds, from `instrument::timing`. */
export const TIMING = {
  PRESS: 0.09,
  LIGHT: 0.16,
  RISE: 0.42,
  STAGGER: 0.03,
  ROLL: 0.6,
  BREATH: 2.4,
  BREATHING_FOR: 12.0,
};

/** Data series colours in order: blue, mint, amber, coral, violet (`Instrument::series`). */
export const SERIES = [
  'var(--accent-blue)',
  'var(--accent-mint)',
  'var(--accent-amber)',
  'var(--accent-coral)',
  'var(--accent-violet)',
];

/** The colour of series `index`, wrapping round the five. */
export function seriesColor(index) {
  return SERIES[((index % SERIES.length) + SERIES.length) % SERIES.length];
}

/** A colour at an opacity from 0 to 1, the Rust `colour.gamma_multiply(a)`. */
export function alpha(colour, a) {
  return `color-mix(in srgb, ${colour} ${Math.max(0, Math.min(1, a)) * 100}%, transparent)`;
}

/** A colour a little brighter, for the thing under the pointer (`brighten`). */
export function brighten(colour) {
  return `color-mix(in srgb, ${colour} 75%, white)`;
}

const clamp01 = (t) => Math.max(0, Math.min(1, t));

/** Ease out, cubic: quick to start and slow to settle. */
export function easeOut(t) {
  const u = clamp01(t);
  return 1 - Math.pow(1 - u, 3);
}

/** Ease out with a small overshoot, for something rising into place. */
export function easeBack(t) {
  const u = clamp01(t);
  const c = 1.2;
  return 1 + (c + 1) * Math.pow(u - 1, 3) + c * Math.pow(u - 1, 2);
}

/** How far through an appearance a clock reading `t` is, from 0 to 1, over `seconds` after `delay`. */
export function progress(t, delay, seconds) {
  return clamp01((t - delay) / Math.max(seconds, 0.001));
}

function prefersStill() {
  return typeof window !== 'undefined' && window.matchMedia
    ? window.matchMedia('(prefers-reduced-motion: reduce)').matches
    : false;
}

/**
 * Seconds since mount, while `active` and shorter than `seconds`; Infinity once it has finished, or
 * when the viewer prefers reduced motion. Components compute their own progress from it with `progress`.
 */
export function useClock(active = true, seconds = 1) {
  const still = prefersStill();
  const [t, setT] = useState(still || !active ? Infinity : 0);
  useEffect(() => {
    if (still || !active) {
      setT(Infinity);
      return undefined;
    }
    let raf;
    let start;
    const tick = (now) => {
      if (start == null) start = now;
      const elapsed = (now - start) / 1000;
      if (elapsed >= seconds) {
        setT(Infinity);
        return;
      }
      setT(elapsed);
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [active, seconds, still]);
  return t;
}

const widths = new Map();

/** The width of `text` in the installed faces, for layout that has to know before it draws. */
export function textWidth(text, { size = 12, family = 'sans', weight = 400, tracking = 0 } = {}) {
  const key = `${family}|${weight}|${size}|${text}`;
  if (widths.has(key)) return widths.get(key) + tracking * size * text.length;
  let w = text.length * size * (family === 'mono' ? 0.6 : 0.55);
  if (typeof document !== 'undefined') {
    const ctx = textWidth.ctx || (textWidth.ctx = document.createElement('canvas').getContext('2d'));
    if (ctx) {
      ctx.font = `${weight} ${size}px ${family === 'mono' ? '"JetBrains Mono"' : 'Inter'}, sans-serif`;
      w = ctx.measureText(text).width;
    }
  }
  widths.set(key, w);
  return w + tracking * size * text.length;
}

/**
 * A raised plate with a lit edge along its top. `radius` and `padding` are points.
 * Rust: `instrument::plate`.
 */
export function Plate({ radius = 12, padding = 12, className = '', style, children, ...rest }) {
  return (
    <div
      className={`rux-plate rux-lit-edge ${className}`.trim()}
      style={{ '--rux-lit-r': `${radius}px`, borderRadius: radius, padding, ...style }}
      {...rest}
    >
      {children}
    </div>
  );
}

/**
 * A recessed screen: the darkest ground there is, with an optional dot graticule.
 * Rust: `instrument::screen`.
 */
export function Screen({ radius = 9, graticule = false, className = '', style, children, ...rest }) {
  const cls = `rux-screen${graticule ? ' rux-screen--graticule' : ''} ${className}`.trim();
  return (
    <div className={cls} style={{ '--rux-screen-r': `${radius}px`, borderRadius: radius, ...style }} {...rest}>
      {children}
    </div>
  );
}

/**
 * An LED: off, or lit in `colour` at `brightness` from 0 to 1, with a bloom of six rings round it.
 * `radius` is the lamp's radius in points. Rust: `instrument::led`.
 */
export function Led({ colour = 'var(--accent-blue)', brightness = 1, radius = 3, className = '', style, ...rest }) {
  const lit = clamp01(brightness);
  const shadows = ['inset 0 0 0 0.6px color-mix(in srgb, black 35%, transparent)'];
  if (lit > 0.01) {
    for (let ring = 1; ring <= 6; ring += 1) {
      shadows.push(`0 0 0 ${radius * 0.42 * ring}px ${alpha(colour, lit * 0.13 * (1 - ring / 7))}`);
    }
  }
  return (
    <span
      className={`rux-led ${className}`.trim()}
      aria-hidden="true"
      style={{
        width: radius * 2,
        height: radius * 2,
        '--rux-led-r': `${radius}px`,
        '--rux-led-glint': `color-mix(in srgb, white ${((40 + 120 * lit) / 255) * 100}%, transparent)`,
        background: `color-mix(in srgb, ${colour} ${lit * 100}%, var(--inst-led-off))`,
        boxShadow: shadows.join(', '),
        ...style,
      }}
      {...rest}
    />
  );
}

/** Silkscreen label printed on a plate: small, spaced capitals (`SILK`). */
export function Silk({ className = '', children, ...rest }) {
  return (
    <span className={`rux-silk ${className}`.trim()} {...rest}>
      {children}
    </span>
  );
}

/** A reading on a screen (`READING`): mono 21, medium, tight. */
export function Readout({ className = '', children, ...rest }) {
  return (
    <span className={`rux-readout ${className}`.trim()} {...rest}>
      {children}
    </span>
  );
}

export const Instrument = { Plate, Screen, Led, Silk, Readout, series: seriesColor };

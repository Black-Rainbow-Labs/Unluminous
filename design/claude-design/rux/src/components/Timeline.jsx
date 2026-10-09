import React from 'react';
import { easeOut, Led, progress, TIMING, useClock } from './Instrument.jsx';

/**
 * A timeline: a groove with an LED for each item. Lit mint for done, a blue lamp that breathes for the one
 * in progress (for 12 seconds, then it holds), dark with a rim for what is to come. Rust `table::Timeline`.
 *
 * `items` is `[{ time, title, text, stage: 'done' | 'active' | 'todo' }]`.
 */
export function Timeline({ items = [], animate = true, className = '', style, ...rest }) {
  const t = useClock(animate, TIMING.BREATHING_FOR);
  return (
    <div className={`rux-timeline ${className}`.trim()} style={style} {...rest}>
      {items.map((item, index) => {
        const stage = item.stage || 'todo';
        const on = easeOut(progress(t, index * TIMING.STAGGER * 3, TIMING.LIGHT * 2));
        let colour = 'var(--ink-300)';
        let lit = 0;
        if (stage === 'done') {
          colour = 'var(--accent-mint)';
          lit = on;
        } else if (stage === 'active') {
          colour = 'var(--accent-blue)';
          const breath = t < TIMING.BREATHING_FOR ? 0.75 + 0.25 * Math.cos((t / TIMING.BREATH) * Math.PI * 2) : 1;
          lit = on * breath;
        }
        return (
          <div key={index} className={`rux-timeline__item is-${stage}`} role="listitem" style={{ display: 'contents' }}>
            <span className={`rux-timeline__time${item.time ? ' has-time' : ''}`}>{item.time}</span>
            <span className="rux-timeline__rail">
              <Led colour={colour} brightness={lit} radius={4} className="rux-timeline__lamp" style={{ transition: 'none' }} />
            </span>
            <span className="rux-timeline__words">
              <span className="rux-timeline__title">{item.title}</span>
              {item.text ? <span className="rux-timeline__text">{item.text}</span> : null}
            </span>
          </div>
        );
      })}
    </div>
  );
}

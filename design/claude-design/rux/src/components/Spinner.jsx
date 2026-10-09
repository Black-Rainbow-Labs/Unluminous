import React, { useId } from 'react';

const cx = (...parts) => parts.filter(Boolean).join(' ');

/** `.ic-elapsed` text: "7s" under a minute, "1m 35s" after (Rust `elapsed_text`). */
export function elapsedText(seconds) {
  const s = Math.floor(seconds);
  return s < 60 ? `${s}s` : `${Math.floor(s / 60)}m ${s % 60}s`;
}

/** The neumorphic spinner: a pressed disc with a turning blue to violet arc (Rust `Spinner`). `bare` drops the disc. */
export function Spinner({ diameter = 56, bare = false, label = 'Loading', className, style, ...rest }) {
  const id = useId();
  const vars = { '--d': `${diameter}px`, '--k': diameter / 56, ...style };
  return (
    <span role="status" aria-label={label} className={cx('rux-spinner', bare && 'rux-spinner--bare', className)} style={vars} {...rest}>
      <svg className="rux-spinner__ring" viewBox="0 0 44 44" aria-hidden="true">
        <defs>
          <linearGradient id={id} x1="0" y1="0" x2="1" y2="0">
            <stop offset="0%" style={{ stopColor: 'var(--grad-multistop-from)' }} />
            <stop offset="100%" style={{ stopColor: 'var(--grad-multistop-to)' }} />
          </linearGradient>
        </defs>
        <circle className="rux-spinner__track" cx="22" cy="22" r="18" fill="none" strokeWidth="3" />
        <path className="rux-spinner__arc" d="M22 4 a18 18 0 0 1 18 18" fill="none" strokeWidth="3" strokeLinecap="round" stroke={`url(#${id})`} />
      </svg>
    </span>
  );
}

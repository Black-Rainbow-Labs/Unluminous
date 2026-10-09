import React from 'react';
import { Icon } from '../icon.jsx';

/**
 * TextArea: a well of running text (Rust `TextArea`; `.ic-prompt-wrap`, `.ic-prompt`, `.ic-form-textarea`).
 * `corner` is a small round button (icon name) in the top right; `count` shows the character count bottom right.
 */
export function TextArea({
  value, defaultValue = '', onChange, hint = '', count = false, corner, onCorner, cornerLabel = 'Regenerate',
  disabled = false, label = 'Text', className = '', style, ...rest
}) {
  const [inner, setInner] = React.useState(defaultValue);
  const controlled = value !== undefined;
  const text = controlled ? value : inner;
  const decorated = Boolean(corner) || count;
  const classes = ['rux-text-area', decorated && 'rux-text-area--decorated', disabled && 'is-disabled', className]
    .filter(Boolean).join(' ');
  return (
    <div className={classes} style={style} {...rest}>
      <textarea
        className="rux-text-area__field" value={text} placeholder={hint} disabled={disabled} aria-label={label}
        onChange={(event) => { if (!controlled) setInner(event.target.value); if (onChange) onChange(event.target.value); }}
      />
      {corner && (
        <button type="button" className="rux-text-area__corner" aria-label={cornerLabel} disabled={disabled} onClick={onCorner}>
          <Icon name={corner} size={14} />
        </button>
      )}
      {count && <span className="rux-text-area__count">{Array.from(text).length}</span>}
    </div>
  );
}

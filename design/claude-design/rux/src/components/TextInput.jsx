import React from 'react';
import { Icon } from '../icon.jsx';

/**
 * TextInput: a one-line field pressed into the surface (Rust `TextInput`; `.ic-form-input`, `.search-field`).
 * `search` makes the pill with a magnifier; `icon` puts any mark before the words; `mono` is for numbers and paths.
 */
export function TextInput({
  value, defaultValue = '', onChange, onSubmit, hint = '', icon, search = false, mono = false,
  disabled = false, label = 'Text', className = '', style, inputProps, ...rest
}) {
  const [inner, setInner] = React.useState(defaultValue);
  const controlled = value !== undefined;
  const text = controlled ? value : inner;
  const mark = icon || (search ? 'search' : null);
  const classes = [
    'rux-text-input', search && 'rux-text-input--search', mono && 'rux-text-input--mono',
    disabled && 'is-disabled', className,
  ].filter(Boolean).join(' ');
  return (
    <label className={classes} style={style} {...rest}>
      {mark && <span className="rux-text-input__mark"><Icon name={mark} size={13} /></span>}
      <input
        className="rux-text-input__field" type="text" value={text} placeholder={hint}
        disabled={disabled} aria-label={label}
        onChange={(event) => { if (!controlled) setInner(event.target.value); if (onChange) onChange(event.target.value); }}
        onKeyDown={(event) => { if (event.key === 'Enter' && onSubmit) onSubmit(text); }}
        {...inputProps}
      />
    </label>
  );
}

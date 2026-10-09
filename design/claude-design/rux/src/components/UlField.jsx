import React from 'react';
import { UlIcon } from './UlIcon.jsx';

const cx = (...parts) => parts.filter(Boolean).join(' ');

/**
 * A text field (controls::search_field, modal::field): field fill, a one point divider stroke, corner
 * radius 6, the words in text_control and the placeholder in text_faint. A search field has a
 * magnifier 13 points in from its left edge.
 * @param search - whether to draw the magnifier
 * @param value - the words in the field
 * @param placeholder - the words before anything is typed
 */
export function UlField({ search = true, value, defaultValue, placeholder = 'Filter files', onChange, className, style, inputProps, ...rest }) {
  return (
    <label className={cx('ul-field', !search && 'ul-field--plain', className)} style={style} {...rest}>
      {search && <UlIcon name="magnifier" size={20} className="ul-field__mark" />}
      <input
        className="ul-field__input"
        placeholder={placeholder}
        value={value}
        defaultValue={value === undefined ? defaultValue : undefined}
        onChange={onChange ? (event) => onChange(event.target.value) : undefined}
        aria-label={placeholder}
        {...inputProps}
      />
    </label>
  );
}

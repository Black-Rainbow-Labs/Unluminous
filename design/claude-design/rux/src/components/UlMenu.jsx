import React from 'react';
import { UlIcon } from './UlIcon.jsx';

const cx = (...parts) => parts.filter(Boolean).join(' ');

/** The File menu as Unluminous draws it on Windows. */
export const UL_SAMPLE_MENU = [
  { label: 'New File', shortcut: 'Ctrl+N' },
  { label: 'Open File...', shortcut: 'Ctrl+O' },
  { label: 'Open Folder...', shortcut: 'Ctrl+Alt+O' },
  { label: 'Open Folder in New Window' },
  { separator: true },
  { heading: 'Recent' },
  { label: 'unluminous' },
  { label: 'inillucent' },
  { separator: true },
  { label: 'Save', shortcut: 'Ctrl+S' },
  { label: 'Save As...', shortcut: 'Ctrl+Shift+S', disabled: true },
  { label: 'Word Wrap', ticked: true },
  { separator: true },
  { label: 'Settings', shortcut: 'Ctrl+,' },
];

/**
 * A menu (controls::menu_rows): menu fill, a one point control_border stroke, a 6 point margin and 340
 * points wide. A row is 24 points: a tick when it is on, the name 18 points in, the shortcut right
 * aligned in text_faint. Every menu in the window, the bar's and every right click menu, is this one.
 * @param items - { label, shortcut, ticked, disabled, chosen, submenu } or { separator } or { heading }
 * @param width - 340 by default
 */
export function UlMenu({ items = UL_SAMPLE_MENU, width = 340, onPick, className, style, ...rest }) {
  return (
    <div className={cx('ul-menu', className)} style={{ width, ...style }} role="menu" {...rest}>
      {items.map((item, index) => {
        if (item.separator) return <div key={index} className="ul-menu__separator" role="separator" />;
        if (item.heading) return <div key={index} className="ul-menu__heading">{item.heading}</div>;
        return (
          <button
            type="button"
            key={index}
            role="menuitem"
            disabled={item.disabled}
            className={cx('ul-menu__row', item.chosen && 'is-chosen')}
            onClick={() => onPick && onPick(item, index)}
          >
            <span className="ul-menu__tick">{item.ticked && <UlIcon name="tick" size={18} />}</span>
            <span className="ul-menu__label">{item.label}</span>
            {item.shortcut && <span className="ul-menu__shortcut">{item.shortcut}</span>}
            {item.submenu && <UlIcon name="disclosure-closed" size={18} className="ul-menu__more" />}
          </button>
        );
      })}
    </div>
  );
}

import React from 'react';
import { UlIcon } from './UlIcon.jsx';

const cx = (...parts) => parts.filter(Boolean).join(' ');

/** The rail's default buttons, top group then bottom group, as Unluminous opens. */
export const UL_RAIL_TOP = [
  { id: 'explorer', icon: 'folder', label: 'Project', on: true },
  { id: 'editor', icon: 'file', label: 'Editing Area', on: true },
  { id: 'git', icon: 'branch', label: 'Git' },
  { id: 'realm', icon: 'realm', label: 'Realm' },
  { id: 'chat', icon: 'chat', label: 'Agent-Chat' },
  { id: 'tasks', icon: 'board', label: 'Agent-Tasks' },
  { id: 'database', icon: 'database', label: 'Database' },
];
export const UL_RAIL_BOTTOM = [
  { id: 'debug', icon: 'bug', label: 'Debug tile' },
  { id: 'run', icon: 'run', label: 'Run tile' },
  { id: 'terminal', icon: 'terminal', label: 'Terminal tile' },
];

/**
 * The thin rail of pane buttons down the far left (components::activity_bar). One button a pane;
 * a pane that is open draws the accent wash and a short accent bar against the rail's left edge.
 * @param top - buttons from the top: { id, icon, label, on, disabled }
 * @param bottom - buttons from the bottom
 * @param onToggle - called with a button's id
 */
export function UlActivityBar({ top = UL_RAIL_TOP, bottom = UL_RAIL_BOTTOM, onToggle, className, style, ...rest }) {
  const button = (item) => (
    <button
      type="button"
      key={item.id}
      className={cx('ul-rail__button', item.on && 'is-on')}
      aria-label={item.label}
      aria-pressed={!!item.on}
      disabled={item.disabled}
      title={item.label}
      onClick={() => onToggle && onToggle(item.id)}
    >
      <UlIcon name={item.icon} size={24} />
    </button>
  );
  return (
    <nav className={cx('ul-rail', className)} style={style} aria-label="Panes" {...rest}>
      <div className="ul-rail__group">{top.map(button)}</div>
      <div className="ul-rail__group">{bottom.map(button)}</div>
    </nav>
  );
}

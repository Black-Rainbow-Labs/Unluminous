import React from 'react';
import { UlIcon } from './UlIcon.jsx';

const cx = (...parts) => parts.filter(Boolean).join(' ');

/**
 * The strip of file tabs above an editing pane (components::file_tabs): 32 points, toolbar fill, the
 * open tab lifted to the editor's colour with a two point accent underline.
 * @param tabs - { name, active, unsaved, transient, badge, mark }
 * @param onPick - called with a tab's index
 * @param onClose - called with a tab's index
 */
export function UlTabStrip({ tabs = [{ name: 'program.rs', active: true, badge: { letter: 'R', colour: '#b7410e' } }], onPick, onClose, className, style, ...rest }) {
  return (
    <div className={cx('ul-tabs', className)} style={style} role="tablist" {...rest}>
      {tabs.map((tab, index) => (
        <div
          key={`${tab.name}-${index}`}
          role="tab"
          aria-selected={!!tab.active}
          className={cx('ul-tabs__tab', tab.active && 'is-active', tab.transient && 'is-transient')}
          onClick={() => onPick && onPick(index)}
        >
          {tab.badge ? (
            <span className="ul-tabs__badge" style={{ background: tab.badge.colour }}>{tab.badge.letter}</span>
          ) : (
            <span className="ul-tabs__dot" />
          )}
          <span className="ul-tabs__name">{tab.name}</span>
          {tab.unsaved ? (
            <span className="ul-tabs__unsaved" aria-label="Unsaved" />
          ) : (
            <button type="button" className="ul-tabs__close" aria-label={`Close ${tab.name}`} onClick={(event) => { event.stopPropagation(); onClose && onClose(index); }}>
              <UlIcon name="cross" size={18} />
            </button>
          )}
        </div>
      ))}
    </div>
  );
}

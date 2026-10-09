import React from 'react';
import { UlIcon } from './UlIcon.jsx';

const cx = (...parts) => parts.filter(Boolean).join(' ');

/** A terminal running claude, the screen a terminal tile shows. */
export const UL_SAMPLE_TERMINAL = [
  { text: 'PS C:\\jason\\dev\\unluminous> cargo test -p unluminous-core', tone: 'prompt' },
  { text: '   Compiling unluminous-core v0.68.1' },
  { text: '    Finished `test` profile [unoptimized + debuginfo] target(s) in 3.59s' },
  { text: '     Running unittests src/lib.rs' },
  { text: 'test result: ok. 1214 passed; 0 failed; 0 ignored', tone: 'ok' },
  { text: 'PS C:\\jason\\dev\\unluminous> ', tone: 'prompt', caret: true },
];

/**
 * A docked panel (components::terminal_panel, the plugin panes): a 32 point header that is also the
 * handle it is dragged by, holding its name and its tabs, and a body. With `terminal` it is the
 * terminal tile, its screen set in the code font on the editor's colour.
 * @param title - the panel's name at the left of the header
 * @param tabs - the header's tabs: { name, active }
 * @param terminal - lines of a terminal screen: { text, tone: "prompt" | "ok" | "error", caret }
 * @param count - a quiet number after the title, as Agent-Tasks and Realm show
 */
export function UlTile({ title = 'Terminal', tabs = [{ name: 'pwsh', active: true }, { name: 'claude' }], terminal = UL_SAMPLE_TERMINAL, count, children, className, style, ...rest }) {
  return (
    <section className={cx('ul-tile', className)} style={style} aria-label={title} {...rest}>
      <header className="ul-tile__header">
        <span className="ul-tile__title">{title}</span>
        {count !== undefined && <span className="ul-tile__count">{count}</span>}
        {tabs.map((tab) => (
          <span key={tab.name} className={cx('ul-tile__tab', tab.active && 'is-active')}>
            {tab.name}
            <UlIcon name="cross" size={16} className="ul-tile__tab-close" />
          </span>
        ))}
        {tabs.length > 0 && <button type="button" className="ul-tile__button" aria-label="New terminal tab"><UlIcon name="plus" size={20} /></button>}
        <span className="ul-tile__spacer" />
        <button type="button" className="ul-tile__button" aria-label={`Hide ${title}`}><UlIcon name="cross" size={20} /></button>
      </header>
      <div className={cx('ul-tile__body', !children && 'ul-tile__body--terminal')}>
        {children ?? terminal.map((line, index) => (
          <div key={index} className={cx('ul-tile__line', line.tone && `is-${line.tone}`)}>
            {line.text}
            {line.caret && <span className="ul-tile__caret" />}
          </div>
        ))}
      </div>
    </section>
  );
}

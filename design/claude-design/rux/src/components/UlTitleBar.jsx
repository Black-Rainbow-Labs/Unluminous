import React from 'react';
import { UlIcon } from './UlIcon.jsx';

const cx = (...parts) => parts.filter(Boolean).join(' ');
const MENUS = ['File', 'Edit', 'Code', 'Find', 'View', 'Run', 'Git', 'Plugins'];
const VIEW_MODES = [
  { id: 'raw', icon: 'view-raw', label: 'Raw' },
  { id: 'side-by-side', icon: 'view-side-by-side', label: 'Side by side' },
  { id: 'preview', icon: 'view-preview', label: 'Preview' },
];

/**
 * The bar along the top of the window: the app name, the menus, the project, the text tools, the run
 * widget and the window buttons (components::title_bar).
 * @param project - the project folder's name, drawn bold after the menus
 * @param textTools - whether the F button and the view modes are drawn (a Markdown or prose file)
 * @param viewMode - which of raw, side-by-side and preview is chosen
 * @param running - whether a run is going, which adds the stop button
 * @param platform - "windows" draws the menus and three round buttons; "macos" draws the lights and no menus
 */
export function UlTitleBar({ appName = 'Unluminous', menus = MENUS, openMenu, project, textTools = true, viewMode = 'raw', debug = true, running = false, platform = 'windows', onViewMode, className, style, ...rest }) {
  return (
    <header className={cx('ul-title-bar', `ul-title-bar--${platform}`, className)} style={style} {...rest}>
      {platform === 'macos' && (
        <div className="ul-title-bar__lights" aria-hidden="true">
          <span className="ul-title-bar__light ul-title-bar__light--close" />
          <span className="ul-title-bar__light ul-title-bar__light--minimise" />
          <span className="ul-title-bar__light ul-title-bar__light--maximise" />
        </div>
      )}
      {platform === 'windows' && (
        <nav className="ul-title-bar__menus" aria-label="Menu bar">
          <span className="ul-title-bar__app">{appName}</span>
          {menus.map((menu) => (
            <button type="button" key={menu} className={cx('ul-title-bar__menu', openMenu === menu && 'is-open')}>{menu}</button>
          ))}
        </nav>
      )}
      {project && <span className="ul-title-bar__project">{project}</span>}
      <div className="ul-title-bar__spacer" />
      {textTools && (
        <div className="ul-title-bar__tools">
          <button type="button" className="ul-title-bar__icon" aria-label="Text options"><UlIcon name="font" size={22} /></button>
          <div className="ul-segments" role="radiogroup" aria-label="View mode">
            {VIEW_MODES.map((mode) => (
              <button
                type="button"
                key={mode.id}
                role="radio"
                aria-checked={viewMode === mode.id}
                aria-label={mode.label}
                className={cx('ul-segments__segment', viewMode === mode.id && 'is-chosen')}
                onClick={() => onViewMode && onViewMode(mode.id)}
              >
                <UlIcon name={mode.icon} size={22} />
              </button>
            ))}
          </div>
        </div>
      )}
      <div className="ul-title-bar__run">
        <button type="button" className="ul-title-bar__icon" aria-label="Run the selected configuration"><UlIcon name="run" size={22} /></button>
        {debug && <button type="button" className="ul-title-bar__icon" aria-label="Debug the selected configuration"><UlIcon name="debug-run" size={22} /></button>}
        {running && <button type="button" className="ul-title-bar__icon ul-title-bar__icon--stop" aria-label="Stop the run"><UlIcon name="stop" size={22} /></button>}
      </div>
      {platform === 'windows' && (
        <div className="ul-title-bar__buttons">
          <button type="button" className="ul-title-bar__window ul-title-bar__window--minimise" aria-label="Minimise" />
          <button type="button" className="ul-title-bar__window ul-title-bar__window--maximise" aria-label="Maximise" />
          <button type="button" className="ul-title-bar__window ul-title-bar__window--close" aria-label="Close" />
        </div>
      )}
    </header>
  );
}

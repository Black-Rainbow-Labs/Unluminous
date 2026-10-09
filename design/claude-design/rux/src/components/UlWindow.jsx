import React from 'react';
import { UlTitleBar } from './UlTitleBar.jsx';
import { UlActivityBar } from './UlActivityBar.jsx';
import { UlExplorer } from './UlExplorer.jsx';
import { UlTabStrip } from './UlTabStrip.jsx';
import { UlEditor } from './UlEditor.jsx';
import { UlStatusBar } from './UlStatusBar.jsx';

const cx = (...parts) => parts.filter(Boolean).join(' ');

/**
 * The whole Unluminous window: a rounded frame (WINDOW_CORNER 12) holding the title bar, the rail, the
 * panels and the editing area, and the status bar. Every part is a prop, so a design can swap one
 * piece and keep the rest; leave a prop out and the window draws Unluminous as it opens.
 * @param titleBar - the bar along the top, or the UlTitleBar props
 * @param rail - the rail of pane buttons
 * @param left - what is docked on the left, the explorer by default; null for nothing
 * @param main - the editing area: the tab strip and the editor by default
 * @param right - what is docked on the right, such as Agent-Chat
 * @param bottom - what is docked along the bottom, such as the terminal tile
 * @param statusBar - the bar along the bottom
 * @param overlay - drawn over the whole window, such as a modal or an open menu
 */
export function UlWindow({ width = 1180, height = 740, titleBar, rail, left, main, right, bottom, bottomHeight = 260, rightWidth = 420, statusBar, overlay, transparent = false, className, style, ...rest }) {
  return (
    <div className={cx('ul-window', transparent && 'is-transparent', className)} style={{ width, height, ...style }} {...rest}>
      {titleBar ?? <UlTitleBar project="unluminous" textTools={false} />}
      <div className="ul-window__body">
        {rail ?? <UlActivityBar />}
        {left === undefined ? <UlExplorer /> : left}
        <div className="ul-window__centre">
          <div className="ul-window__row">
            <div className="ul-window__main">
              {main ?? (
                <>
                  <UlTabStrip />
                  <UlEditor />
                </>
              )}
            </div>
            {right && <div className="ul-window__right" style={{ width: rightWidth }}>{right}</div>}
          </div>
          {bottom && <div className="ul-window__bottom" style={{ height: bottomHeight }}>{bottom}</div>}
        </div>
      </div>
      {statusBar ?? <UlStatusBar />}
      {overlay}
    </div>
  );
}

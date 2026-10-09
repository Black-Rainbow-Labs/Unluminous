import React from 'react';
import { Panel } from './Panel.jsx';

export default function PanelPreview() {
  return (
    <div className="rux-stage" style={{ display: 'grid', gap: 24, gridTemplateColumns: 'repeat(3, 220px)', padding: 24 }}>
      <Panel>
        <div className="rux-t-panel-title">Raised</div>
        <p className="rux-t-prose">Prompts</p>
      </Panel>
      <Panel kind="raisedSmall" radius={18} pad={16}>
        <div className="rux-t-panel-title">Raised small</div>
        <p className="rux-t-prose">Start Work</p>
      </Panel>
      <Panel kind="flat" fill="var(--surface-2)">
        <div className="rux-t-panel-title">Flat</div>
        <p className="rux-t-prose">claude</p>
      </Panel>
    </div>
  );
}

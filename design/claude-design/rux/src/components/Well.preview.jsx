import React from 'react';
import { Well } from './Well.jsx';
import { Panel } from './Panel.jsx';

export default function WellPreview() {
  return (
    <div className="rux-stage" style={{ padding: 24 }}>
      <Panel style={{ width: 520, display: 'grid', gap: 16 }}>
        <Well pad={16} style={{ height: 80 }}><span className="rux-t-prose">Deep well: timeline</span></Well>
        <Well shallow pad={16} style={{ height: 80 }}><span className="rux-t-prose">Shallow well: tile</span></Well>
        <Well radius="var(--r-lg)" pad="12px 18px"><span className="rux-t-prose">Larger radius</span></Well>
      </Panel>
    </div>
  );
}

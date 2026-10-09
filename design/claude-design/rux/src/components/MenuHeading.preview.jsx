import React from 'react';
import { MenuHeading } from './MenuHeading.jsx';
import { ListItem } from './ListItem.jsx';

export default function MenuHeadingPreview() {
  return (
    <div className="rux-stage" style={{ padding: 24 }}>
      <div style={{ width: 260, background: 'var(--surface-1)', borderRadius: 'var(--r-lg)', boxShadow: 'var(--e-raised)', padding: 8 }}>
        <MenuHeading>Recent projects</MenuHeading>
        <ListItem label="Unluminous" dot selected tick />
        <ListItem label="Sable" dot />
        <MenuHeading>Archived</MenuHeading>
        <ListItem label="Chordical" dot />
      </div>
    </div>
  );
}

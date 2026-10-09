import React from 'react';
import { IconButton } from './IconButton.jsx';

export default function IconButtonPreview() {
  const row = { display: 'flex', gap: 16, alignItems: 'center', flexWrap: 'wrap', marginBottom: 24 };
  return (
    <div className="rux-stage" style={{ padding: 24 }}>
      <div style={row}>
        <IconButton icon="chevDown" label="Collapse" size="tiny" />
        <IconButton icon="wand" label="Shuffle prompt" size="sm" />
        <IconButton icon="x" label="Close" size="md" />
        <IconButton icon="settings" label="Settings" size="lg" />
        <IconButton icon="plus" label="New" size="xl" />
      </div>
      <div style={row}>
        <IconButton icon="bell" label="Notifications" dot="var(--success)" />
        <IconButton icon="layers" label="Layers" variant="sunken" />
        <IconButton icon="plus" label="Create" variant="primary" size="xl" badge={3} />
        <IconButton icon="dots" label="More" variant="ghost" />
        <IconButton icon="spark" label="Spark" tint="var(--accent-amber)" />
        <IconButton icon="chevDown" label="Expand" size="tiny" turn={Math.PI} />
        <IconButton icon="trash" label="Delete" disabled />
      </div>
    </div>
  );
}

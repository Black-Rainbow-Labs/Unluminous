import React from 'react';
import { Segmented } from './Segmented.jsx';

export default function SegmentedPreview() {
  return (
    <div className="rux-stage" style={{ padding: 24, display: 'grid', gap: 24, justifyItems: 'start' }}>
      <Segmented items={[{ label: 'Grid', icon: 'layers' }, { label: 'Storyboard', icon: 'storyboard' }, { label: 'List' }]} />
      <Segmented
        compact
        defaultValue={1}
        items={[
          { label: 'Image', icon: 'image', accent: 'var(--accent-blue)' },
          { label: 'Video', icon: 'video', accent: 'var(--accent-coral)' },
          { label: 'Music', icon: 'music', accent: 'var(--accent-violet)' },
          { label: 'Prompts', icon: 'docs', accent: 'var(--accent-mint)' },
        ]}
      />
    </div>
  );
}

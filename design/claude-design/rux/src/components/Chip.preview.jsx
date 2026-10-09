import React, { useState } from 'react';
import { Chip } from './Chip.jsx';

export default function ChipPreview() {
  const [presets, setPresets] = useState(['Cinematic', 'Studio light', 'Wide']);
  const row = { display: 'flex', gap: 12, alignItems: 'center', flexWrap: 'wrap', marginBottom: 20 };
  return (
    <div className="rux-stage" style={{ padding: 24 }}>
      <div style={row}>
        <Chip label="Size" value="1024 x 576" />
        <Chip mono label="Volume" dot="var(--success)" />
        <Chip label="Draft" icon="edit" />
        <Chip variant="raised" mono label="1m 35s" />
        <Chip mono label="Save to" icon="folder" />
      </div>
      <div style={row}>
        {presets.map((p) => (
          <Chip key={p} variant="accent" label={p} dismissible onDismiss={() => setPresets(presets.filter((x) => x !== p))} />
        ))}
        <Chip variant="accent" accent="violet" label="Style" />
        <Chip variant="accent" accent="primary" label="Active" />
      </div>
    </div>
  );
}

import React from 'react';
import { Spinner, elapsedText } from './Spinner.jsx';
import { Chip } from './Chip.jsx';

export default function SpinnerPreview() {
  return (
    <div className="rux-stage" style={{ padding: 24, display: 'flex', gap: 28, alignItems: 'center' }}>
      <Spinner />
      <Spinner diameter={32} />
      <Spinner diameter={20} bare />
      <div style={{ display: 'grid', gap: 8, justifyItems: 'center' }}>
        <Spinner />
        <Chip variant="raised" mono label={elapsedText(95)} />
      </div>
    </div>
  );
}

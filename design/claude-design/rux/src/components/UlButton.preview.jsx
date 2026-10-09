import React from 'react';
import { UlButton } from './UlButton.jsx';

export const height = 100;

export default function UlButtonPreview() {
  return (
    <div className="ul-preview">
      <div style={{ display: 'flex', gap: 10, alignItems: 'center', padding: 16 }}>
      <UlButton label="Cancel" />
      <UlButton label="Done" variant="primary" />
      <UlButton label="Single" variant="choice" on />
      <UlButton label="1.5" variant="choice" />
      <UlButton label="In use" variant="state" />
      <UlButton label="Commit" disabled />
    </div>
    </div>
  );
}

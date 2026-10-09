import React from 'react';
import { UlField } from './UlField.jsx';

export const height = 120;

export default function UlFieldPreview() {
  return (
    <div className="ul-preview">
      <div style={{ display: 'flex', flexDirection: 'column', gap: 12, padding: 16, width: 280 }}>
      <UlField />
      <UlField search={false} placeholder="Add a todo" />
    </div>
    </div>
  );
}

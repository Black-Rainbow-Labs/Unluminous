import React from 'react';
import { UlIcon, UL_ICON_NAMES } from './UlIcon.jsx';

export const height = 420;

/** Every Unluminous mark on the explorer's ground, named. */
export default function UlIconPreview() {
  return (
    <div className="ul-preview" style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(96px, 1fr))', gap: 4, padding: 16 }}>
      {UL_ICON_NAMES.map((name) => (
        <div key={name} style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 4, padding: '8px 2px' }}>
          <UlIcon name={name} size={36} style={{ color: 'var(--ul-text-control)' }} />
          <span style={{ fontFamily: 'var(--font-mono)', fontSize: 9.5, color: 'var(--ul-text-dim)' }}>{name}</span>
        </div>
      ))}
    </div>
  );
}

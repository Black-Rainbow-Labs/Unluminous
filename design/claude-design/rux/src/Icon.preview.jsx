import React from 'react';
import { Icon, ICON_NAMES } from './icon.jsx';

export const height = 300;

/** Every rux mark at the size a control uses, named. */
export default function IconPreview() {
  return (
    <div className="rux-stage" style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(92px, 1fr))', gap: 8 }}>
      {ICON_NAMES.map((name) => (
        <div key={name} style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 8, padding: '12px 4px', color: 'var(--ink-700)' }}>
          <Icon name={name} size={20} />
          <span style={{ fontFamily: 'var(--font-mono)', fontSize: 10, color: 'var(--ink-500)' }}>{name}</span>
        </div>
      ))}
    </div>
  );
}

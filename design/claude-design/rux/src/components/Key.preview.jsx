import React, { useState } from 'react';
import { Key } from './Key.jsx';
import { Plate, Silk } from './Instrument.jsx';

export default function KeyPreview() {
  const [crate, setCrate] = useState('app');
  return (
    <div className="rux-stage" style={{ display: 'flex', flexDirection: 'column', gap: 20, padding: 24, background: 'var(--surface-1)' }}>
      <Plate radius={14} padding={16}>
        <Silk style={{ color: 'var(--ink-400)' }}>Crate</Silk>
        <div style={{ display: 'flex', gap: 8, marginTop: 10 }}>
          {['core', 'app', 'cli', 'terminal'].map((name) => (
            <Key
              key={name}
              label={name}
              chosen={{ colour: 'var(--accent-blue)', on: crate === name }}
              onClick={() => setCrate(name)}
            />
          ))}
        </div>
      </Plate>
      <Plate radius={14} padding={16}>
        <Silk style={{ color: 'var(--ink-400)' }}>Actions</Silk>
        <div style={{ display: 'flex', gap: 8, marginTop: 10, flexWrap: 'wrap', alignItems: 'center' }}>
          <Key label="Build" />
          <Key label="Release" tinted="var(--accent-blue)" />
          <Key label="Watching" led={{ colour: 'var(--accent-mint)', lit: true }} />
          <Key label="Idle" led={{ colour: 'var(--accent-mint)', lit: false }} />
          <Key label="Held" down />
          <Key label="Disabled" disabled />
          <Key label="Save" compact />
          <Key label="Live" compact led={{ colour: 'var(--accent-coral)', lit: true }} />
        </div>
      </Plate>
    </div>
  );
}

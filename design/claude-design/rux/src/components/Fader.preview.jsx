import React, { useState } from 'react';
import { Fader } from './Fader.jsx';
import { Plate, Silk, Readout } from './Instrument.jsx';

export default function FaderPreview() {
  const [temp, setTemp] = useState(0.7);
  return (
    <div className="rux-stage" style={{ padding: 24, background: 'var(--surface-1)' }}>
      <Plate radius={14} padding={16} style={{ width: 340 }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline' }}>
          <Silk style={{ color: 'var(--ink-400)' }}>Temperature</Silk>
          <Readout style={{ color: 'var(--ink-900)', fontSize: 16 }}>{temp.toFixed(1)}</Readout>
        </div>
        <div style={{ margin: '12px 0 18px' }}>
          <Fader value={temp} min={0} max={1} step={0.1} label="Temperature" onChange={setTemp} />
        </div>
        <Silk style={{ color: 'var(--ink-400)' }}>Max tool rounds</Silk>
        <div style={{ margin: '12px 0 18px' }}>
          <Fader defaultValue={40} min={0} max={100} step={5} colour="var(--accent-mint)" label="Max tool rounds" />
        </div>
        <Silk style={{ color: 'var(--ink-400)' }}>Empty and full</Silk>
        <div style={{ display: 'flex', gap: 16, marginTop: 12 }}>
          <Fader defaultValue={0} label="Empty" />
          <Fader defaultValue={1} colour="var(--accent-amber)" label="Full" />
        </div>
      </Plate>
    </div>
  );
}

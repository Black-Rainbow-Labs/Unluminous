import React from 'react';
import { Meter } from './Meter.jsx';
import { Plate, Silk, Readout } from './Instrument.jsx';

export default function MeterPreview() {
  return (
    <div className="rux-stage" style={{ padding: 24, background: 'var(--surface-1)' }}>
      <Plate radius={14} padding={16} style={{ width: 360 }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline' }}>
          <Silk style={{ color: 'var(--ink-400)' }}>Token usage</Silk>
          <Readout style={{ color: 'var(--ink-900)', fontSize: 16 }}>71.2k / 96k</Readout>
        </div>
        <div style={{ margin: '10px 0 16px' }}>
          <Meter fraction={0.74} top="var(--accent-amber)" animate label="Token usage" />
        </div>
        <Silk style={{ color: 'var(--ink-400)' }}>Context nearly full</Silk>
        <div style={{ margin: '10px 0 16px' }}>
          <Meter fraction={0.96} colour="var(--accent-mint)" top="var(--accent-coral)" label="Context" />
        </div>
        <Silk style={{ color: 'var(--ink-400)' }}>Cache hits, empty and low</Silk>
        <div style={{ display: 'grid', gap: 8, marginTop: 10 }}>
          <Meter fraction={0} label="Empty" />
          <Meter fraction={0.2} colour="var(--accent-violet)" label="Low" />
        </div>
      </Plate>
    </div>
  );
}

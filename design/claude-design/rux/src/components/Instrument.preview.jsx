import React from 'react';
import { Plate, Screen, Led, Silk, Readout, seriesColor } from './Instrument.jsx';

export default function InstrumentPreview() {
  return (
    <div className="rux-stage" style={{ display: 'flex', gap: 24, flexWrap: 'wrap', padding: 24, background: 'var(--surface-1)' }}>
      <Plate radius={14} padding={16} style={{ width: 260 }}>
        <Silk style={{ color: 'var(--ink-400)' }}>Build</Silk>
        <div style={{ margin: '10px 0' }}>
          <Screen graticule radius={9} style={{ padding: '12px 14px', display: 'flex', alignItems: 'baseline', gap: 8 }}>
            <Readout style={{ color: 'var(--ink-900)' }}>41.8</Readout>
            <Silk style={{ color: 'var(--ink-400)' }}>s release</Silk>
          </Screen>
        </div>
        <div style={{ display: 'flex', gap: 14, alignItems: 'center' }}>
          {['core', 'app', 'cli'].map((name, i) => (
            <span key={name} style={{ display: 'inline-flex', gap: 6, alignItems: 'center', font: '500 12px var(--font-sans)', color: 'var(--ink-700)' }}>
              <Led colour={seriesColor(i)} brightness={i === 2 ? 0 : 1} />
              {name}
            </span>
          ))}
        </div>
      </Plate>
      <Plate radius={14} padding={16}>
        <Silk style={{ color: 'var(--ink-400)' }}>LED brightness</Silk>
        <div style={{ display: 'flex', gap: 16, marginTop: 14, padding: 6 }}>
          {[0, 0.25, 0.5, 0.75, 1].map((b) => (
            <Led key={b} colour="var(--accent-mint)" brightness={b} radius={4} />
          ))}
        </div>
      </Plate>
    </div>
  );
}

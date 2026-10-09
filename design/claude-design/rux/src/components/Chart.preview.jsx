import React from 'react';
import { Chart } from './Chart.jsx';
import { Plate, Silk } from './Instrument.jsx';

const crates = ['core', 'app', 'cli', 'terminal', 'git', 'dap'];
const seconds = (v) => `${v}s`;

export default function ChartPreview() {
  return (
    <div className="rux-stage" style={{ display: 'grid', gap: 20, padding: 24, background: 'var(--surface-1)', maxWidth: 560 }}>
      <Plate radius={14} padding={16}>
        <Silk style={{ color: 'var(--ink-400)' }}>Build time per crate</Silk>
        <div style={{ marginTop: 10 }}>
          <Chart
            kind="bar"
            labels={crates}
            format={seconds}
            series={[
              { name: 'debug', values: [12, 41, 6, 9, 8, 5] },
              { name: 'release', values: [28, 96, 11, 17, 15, 9], colour: 'var(--accent-amber)' },
            ]}
          />
        </div>
      </Plate>
      <Plate radius={14} padding={16}>
        <Silk style={{ color: 'var(--ink-400)' }}>Suite duration over releases</Silk>
        <div style={{ marginTop: 10 }}>
          <Chart
            kind="line"
            labels={['0.64', '0.65', '0.66', '0.67', '0.68']}
            format={(v) => `${v}m`}
            series={[{ name: 'window suite', values: [14, 16, 15, 19, 18], colour: 'var(--accent-mint)' }]}
          />
        </div>
      </Plate>
      <Plate radius={14} padding={16}>
        <Silk style={{ color: 'var(--ink-400)' }}>Tokens per turn, stacked area</Silk>
        <div style={{ marginTop: 10 }}>
          <Chart
            kind="area"
            stacked
            labels={['1', '2', '3', '4', '5', '6']}
            format={(v) => `${v}k`}
            series={[
              { name: 'in', values: [8, 14, 19, 22, 31, 34] },
              { name: 'out', values: [2, 3, 5, 4, 6, 7], colour: 'var(--accent-coral)' },
            ]}
          />
        </div>
      </Plate>
      <Plate radius={14} padding={16}>
        <Silk style={{ color: 'var(--ink-400)' }}>Where release time goes</Silk>
        <div style={{ marginTop: 10 }}>
          <Chart kind="donut" labels={['compile', 'tests', 'macOS notarise', 'publish']} series={[{ name: 'share', values: [96, 61, 140, 18] }]} />
        </div>
      </Plate>
    </div>
  );
}

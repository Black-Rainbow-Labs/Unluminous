import React from 'react';
import { Checkbox } from './Checkbox.jsx';
import { Plate, Silk } from './Instrument.jsx';

export default function CheckboxPreview() {
  return (
    <div className="rux-stage" style={{ padding: 24, background: 'var(--surface-1)' }}>
      <Plate radius={14} padding={16} style={{ width: 320 }}>
        <Silk style={{ color: 'var(--ink-400)' }}>Before release</Silk>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 10, marginTop: 12 }}>
          <Checkbox defaultChecked label="Run the window suite with --no-fail-fast" />
          <Checkbox defaultChecked label="Bump the version in Cargo.toml" />
          <Checkbox label="Notarise Unluminous.app and check the stapled ticket on a second machine before publishing" />
          <Checkbox label="Check for updates at startup" settles={false} defaultChecked colour="var(--accent-blue)" />
        </div>
      </Plate>
    </div>
  );
}

import React from 'react';
import { Select } from './Select.jsx';

const models = ['claude-opus', 'claude-sonnet', 'local-qwen 27B', 'codex'];

export default function SelectPreview() {
  const [a, setA] = React.useState(1);
  const [b, setB] = React.useState(null);
  return (
    <div className="rux-stage" style={{ display: 'grid', gap: 16, gridTemplateColumns: 'repeat(2, 220px)', padding: 24, minHeight: 380, alignContent: 'start' }}>
      <Select label="Model" options={models} value={a} onChange={setA} />
      <Select label="Endpoint" options={models} value={b} onChange={setB} placeholder="No endpoint" />
      <Select label="Dimensions" mono options={['1024 x 1024', '1344 x 768', '768 x 1344']} value={0} />
      <Select label="Disabled" disabled options={models} value={0} />
      <Select label="Open" open options={models} value={a} onChange={setA} />
      <Select label="Open, quiet menu" open menuElevation="quiet" options={models} value={0} />
      <Select label="Zoomed" zoom={1.4} options={models} value={2} />
    </div>
  );
}

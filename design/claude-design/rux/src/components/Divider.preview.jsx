import React from 'react';
import { Divider } from './Divider.jsx';

export default function DividerPreview() {
  return (
    <div className="rux-stage" style={{ display: 'grid', gap: 24, width: 420, padding: 24 }}>
      <div><div className="rux-t-caption">Plain</div><Divider /></div>
      <div><div className="rux-t-caption">Faded</div><Divider faded /></div>
      <div><div className="rux-t-caption">Sunken colour</div><Divider color="var(--surface-sunken)" /></div>
    </div>
  );
}

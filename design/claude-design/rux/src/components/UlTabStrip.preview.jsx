import React from 'react';
import { UlTabStrip } from './UlTabStrip.jsx';

export const height = 110;

export default function UlTabStripPreview() {
  return (
    <div className="ul-preview">
      <div style={{ padding: 16 }}><UlTabStrip tabs={[
      { name: 'program.rs', active: true, badge: { letter: 'R', colour: '#b7410e' } },
      { name: 'README.md', unsaved: true },
      { name: 'notes.txt', transient: true },
    ]} /></div>
    </div>
  );
}

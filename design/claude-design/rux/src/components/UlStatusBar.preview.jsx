import React from 'react';
import { UlStatusBar } from './UlStatusBar.jsx';

export const height = 110;

export default function UlStatusBarPreview() {
  return (
    <div className="ul-preview">
      <div style={{ display: 'flex', flexDirection: 'column', gap: 12, padding: 16 }}>
      <UlStatusBar />
      <UlStatusBar items={['untitled', { text: 'Unsaved', unsaved: true }, 'Plain text', 'Ln 1, Col 1']} message="task-2 moved to AGENT DONE" right="Arial · 16 pt" />
    </div>
    </div>
  );
}

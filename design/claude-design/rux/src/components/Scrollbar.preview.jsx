import React from 'react';
import { Scrollbar } from './Scrollbar.jsx';

const projects = ['Storyboard', 'Prompts', 'Characters', 'Scenes', 'Soundtrack', 'Exports', 'Archive', 'Drafts', 'Shared with me', 'Trash'];

export default function ScrollbarPreview() {
  return (
    <div className="rux-stage" style={{ padding: 24 }}>
      <Scrollbar style={{ height: 180, width: 280, padding: '0 6px 0 0' }}>
        {projects.map((name) => (
          <div key={name} className="rux-t-base" style={{ padding: '10px 14px', color: 'var(--ink-700)' }}>{name}</div>
        ))}
      </Scrollbar>
    </div>
  );
}

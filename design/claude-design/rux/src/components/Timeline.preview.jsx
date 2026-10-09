import React from 'react';
import { Timeline } from './Timeline.jsx';
import { Plate, Silk } from './Instrument.jsx';

export default function TimelinePreview() {
  return (
    <div className="rux-stage" style={{ display: 'flex', gap: 20, flexWrap: 'wrap', padding: 24, background: 'var(--surface-1)' }}>
      <Plate radius={14} padding={16} style={{ width: 380 }}>
        <Silk style={{ color: 'var(--ink-400)' }}>Release 0.68.2</Silk>
        <div style={{ marginTop: 12 }}>
          <Timeline
            items={[
              { time: '14:02', title: 'Run the window suite', text: 'Fourteen binaries, --no-fail-fast', stage: 'done' },
              { time: '14:19', title: 'Build the Windows installer', stage: 'done' },
              { time: '14:31', title: 'Notarise Unluminous.app', text: 'Waiting on Apple', stage: 'active' },
              { time: '14:40', title: 'Publish to GitHub and unluminous.com', stage: 'todo' },
            ]}
          />
        </div>
      </Plate>
      <Plate radius={14} padding={16} style={{ width: 260 }}>
        <Silk style={{ color: 'var(--ink-400)' }}>No times</Silk>
        <div style={{ marginTop: 12 }}>
          <Timeline
            items={[
              { title: 'Index built', stage: 'done' },
              { title: 'Searching', text: 'trigram candidates verified by grep-searcher', stage: 'active' },
              { title: 'Answer', stage: 'todo' },
            ]}
          />
        </div>
      </Plate>
    </div>
  );
}

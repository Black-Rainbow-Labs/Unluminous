import React from 'react';
import { SubGroup } from './SubGroup.jsx';
import { Panel } from './Panel.jsx';
import { TextInput } from './TextInput.jsx';

export default function SubGroupPreview() {
  return (
    <div className="rux-stage" style={{ padding: 24 }}>
      <Panel style={{ width: 340 }}>
        <SubGroup first title="Qualities" icon="spark" accent="violet">
          <TextInput label="Style" hint="Cinematic, painterly" />
        </SubGroup>
        <SubGroup title="Scene" icon="image" accent="blue">
          <TextInput label="Setting" hint="Harbour at dawn" />
        </SubGroup>
        <SubGroup title="Lighting" icon="star" accent="amber" defaultOpen={false} />
        <SubGroup title="Person" icon="user" accent="rose" defaultOpen={false} />
      </Panel>
    </div>
  );
}

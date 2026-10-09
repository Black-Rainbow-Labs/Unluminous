import React from 'react';
import { ListItem } from './ListItem.jsx';

const names = ['Sable', 'Unluminous', 'Black Rainbow Labs', 'Chordical'];

export default function ListItemPreview() {
  const [a, setA] = React.useState(1);
  const [b, setB] = React.useState(0);
  return (
    <div className="rux-stage" style={{ display: 'grid', gap: 24, gridTemplateColumns: 'repeat(3, 240px)', padding: 24, alignItems: 'start' }}>
      <div role="listbox" aria-label="Projects" style={{ display: 'grid', gap: 2 }}>
        {names.map((name, i) => <ListItem key={name} label={name} dot tick selected={a === i} onClick={() => setA(i)} />)}
      </div>
      <div role="listbox" aria-label="Prompts" style={{ display: 'grid', gap: 2 }}>
        {['Portrait, soft light', 'Product on marble', 'Storyboard frame'].map((name, i) => (
          <ListItem key={name} label={name} pressedWhenSelected selected={b === i}
            action={{ icon: 'check', label: 'Apply ' + name }} onClick={() => setB(i)} />
        ))}
      </div>
      <div role="listbox" aria-label="Plain" style={{ display: 'grid', gap: 2 }}>
        <ListItem label="Idle row" />
        <ListItem label="Selected row" selected />
      </div>
    </div>
  );
}

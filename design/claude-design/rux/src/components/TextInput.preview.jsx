import React from 'react';
import { TextInput } from './TextInput.jsx';

export default function TextInputPreview() {
  const [name, setName] = React.useState('Start Work');
  return (
    <div className="rux-stage" style={{ display: 'grid', gap: 16, width: 320, padding: 24 }}>
      <TextInput label="Name" value={name} onChange={setName} hint="Project name" />
      <TextInput label="Empty" hint="Untitled prompt" />
      <TextInput label="Filter" search hint="Filter files" />
      <TextInput label="Path" mono icon="folder" defaultValue="C:/jason/dev/unluminous" />
      <TextInput label="Disabled" disabled defaultValue="claude" />
    </div>
  );
}

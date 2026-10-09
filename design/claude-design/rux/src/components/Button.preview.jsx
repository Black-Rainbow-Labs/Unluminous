import React from 'react';
import { Button } from './Button.jsx';

export default function ButtonPreview() {
  const row = { display: 'flex', gap: 12, alignItems: 'center', flexWrap: 'wrap', marginBottom: 20 };
  return (
    <div className="rux-stage" style={{ padding: 24 }}>
      <div style={row}>
        <Button variant="primary" size="lg" icon="spark" label="Create" />
        <Button variant="primary" size="md" icon="spark" label="Render" />
        <Button variant="mint" label="Save" icon="check" />
        <Button label="Prompts" icon="docs" />
        <Button variant="raised" size="md" icon="plus" label="Add Scene" />
        <Button variant="danger" label="Delete" icon="trash" />
        <Button variant="dashed" label="Add Person" icon="plus" />
        <Button variant="ghost" label="More" trailing="chevDown" />
      </div>
      <div style={row}>
        <Button size="mini" label="Mini" />
        <Button size="sm" label="Small" />
        <Button size="md" label="Medium" />
        <Button size="lg" label="Large" />
        <Button disabled label="Disabled" />
        <Button variant="primary" disabled label="Create" />
      </div>
      <div style={{ ...row, maxWidth: 360 }}>
        <Button stretch label="Cancel" />
        <Button stretch variant="primary" label="Create" />
      </div>
    </div>
  );
}

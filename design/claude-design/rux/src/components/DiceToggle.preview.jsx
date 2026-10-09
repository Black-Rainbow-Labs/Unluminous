import React from 'react';
import { DiceToggle } from './DiceToggle.jsx';

export default function DiceTogglePreview() {
  return (
    <div className="rux-stage" style={{ padding: 24, display: 'flex', gap: 20, alignItems: 'center' }}>
      <DiceToggle label="Randomize prompt" />
      <DiceToggle defaultOn label="Randomize seed" />
      <DiceToggle icon="wand" label="Shuffle" />
      <DiceToggle disabled label="Randomize" />
    </div>
  );
}

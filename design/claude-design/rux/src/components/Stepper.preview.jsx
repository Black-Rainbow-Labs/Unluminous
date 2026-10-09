import React from 'react';
import { Stepper } from './Stepper.jsx';

export default function StepperPreview() {
  return (
    <div className="rux-stage" style={{ padding: 24, display: 'flex', gap: 24, alignItems: 'center' }}>
      <Stepper defaultValue={4} min={1} max={8} label="Images" />
      <Stepper defaultValue={1} min={1} max={8} label="Seeds" />
      <Stepper defaultValue={8} min={1} max={8} label="Frames" />
      <Stepper defaultValue={3} min={1} max={8} label="Steps" disabled />
    </div>
  );
}

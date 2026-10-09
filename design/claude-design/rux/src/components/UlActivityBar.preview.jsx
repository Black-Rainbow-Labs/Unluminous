import React from 'react';
import { UlActivityBar } from './UlActivityBar.jsx';

export const height = 360;

export default function UlActivityBarPreview() {
  return (
    <div className="ul-preview">
      <div style={{ display: 'flex', height: 340 }}><UlActivityBar /></div>
    </div>
  );
}

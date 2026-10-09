import React from 'react';
import { UlMenu } from './UlMenu.jsx';

export const height = 400;

export default function UlMenuPreview() {
  return (
    <div className="ul-preview">
      <div style={{ padding: 16 }}><UlMenu /></div>
    </div>
  );
}

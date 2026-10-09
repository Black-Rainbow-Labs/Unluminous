import React from 'react';
import { UlTile } from './UlTile.jsx';

export const height = 260;

export default function UlTilePreview() {
  return (
    <div className="ul-preview">
      <div style={{ display: 'flex', height: 240 }}><UlTile /></div>
    </div>
  );
}

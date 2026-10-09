import React from 'react';
import { UlTitleBar } from './UlTitleBar.jsx';

export const height = 120;

export default function UlTitleBarPreview() {
  return (
    <div className="ul-preview">
      <div style={{ display: 'flex', flexDirection: 'column', gap: 16, padding: 16 }}>
      <UlTitleBar project="unluminous" viewMode="side-by-side" />
      <UlTitleBar project="unluminous" textTools={false} running />
    </div>
    </div>
  );
}

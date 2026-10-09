import React from 'react';
import { UlExplorer, UL_SAMPLE_TREE } from './UlExplorer.jsx';

export const height = 420;

export default function UlExplorerPreview() {
  return (
    <div className="ul-preview">
      <div style={{ display: 'flex', height: 400 }}><UlExplorer project="unluminous-screenshot-folder" rows={[...UL_SAMPLE_TREE.slice(0, 6), { ...UL_SAMPLE_TREE[6], cursor: true }, UL_SAMPLE_TREE[7]]} keyboard /></div>
    </div>
  );
}

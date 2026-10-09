import React from 'react';
import { UlEditor } from './UlEditor.jsx';

export const height = 300;

export default function UlEditorPreview() {
  return (
    <div className="ul-preview">
      <div style={{ display: 'flex', height: 300 }}><UlEditor caretLine={4} breakpoints={[5]} /></div>
    </div>
  );
}

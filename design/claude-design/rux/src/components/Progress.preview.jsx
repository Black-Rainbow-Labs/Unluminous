import React from 'react';
import { Progress } from './Progress.jsx';

export default function ProgressPreview() {
  return (
    <div className="rux-stage" style={{ padding: 24, display: 'grid', gap: 20, maxWidth: 420 }}>
      <Progress fraction={0.25} label="Uploading" />
      <Progress fraction={0.62} label="Rendering" />
      <Progress fraction={1} label="Done" />
      <Progress fraction={0.4} height={10} label="Thick" />
      <Progress label="Waiting for the queue" />
    </div>
  );
}

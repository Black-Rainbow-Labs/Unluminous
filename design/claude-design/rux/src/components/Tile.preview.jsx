import React from 'react';
import { Tile } from './Tile.jsx';
import { Spinner } from './Spinner.jsx';

export default function TilePreview() {
  return (
    <div className="rux-stage" style={{ padding: 24, display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(260px, 1fr))', gap: 24 }}>
      <Tile kind="dropzone" label="Add a reference image" title="Drop an image" subtitle="PNG or JPG up to 10 MB" />
      <Tile kind="well" label="Result">
        <span style={{ display: 'grid', placeItems: 'center', height: '100%' }}><Spinner /></span>
      </Tile>
      <Tile kind="well" label="Empty result" />
    </div>
  );
}

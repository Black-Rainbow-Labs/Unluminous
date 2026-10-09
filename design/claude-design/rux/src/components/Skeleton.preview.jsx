import React from 'react';
import { Skeleton } from './Skeleton.jsx';

export default function SkeletonPreview() {
  return (
    <div className="rux-stage" style={{ padding: 24, display: 'grid', gap: 12, maxWidth: 360 }}>
      <Skeleton style={{ height: 180 }} radius="var(--r-lg)" />
      <Skeleton style={{ height: 14, width: '70%' }} radius="var(--r-pill)" />
      <Skeleton style={{ height: 14, width: '45%' }} radius="var(--r-pill)" />
      <div style={{ display: 'flex', gap: 12, alignItems: 'center' }}>
        <Skeleton style={{ width: 40, height: 40 }} radius="50%" />
        <Skeleton style={{ height: 14 }} radius="var(--r-pill)" />
      </div>
    </div>
  );
}

import React from 'react';
import { UlModal, UlField } from './UlModal.jsx';

export const height = 420;

export default function UlModalPreview() {
  return (
    <div className="ul-preview">
      <div style={{ position: 'relative', height: 400 }}>
      <UlModal title="Rename Symbol" note="4 references in 3 files" buttons={['Cancel', 'Rename']} width={460} height={240}>
        <div style={{ padding: 20, display: 'flex', flexDirection: 'column', gap: 10 }}>
          <span>New name for <b>definitions</b></span>
          <UlField search={false} defaultValue="definitions_in" />
        </div>
      </UlModal>
    </div>
    </div>
  );
}

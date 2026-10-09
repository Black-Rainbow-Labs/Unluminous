import React from 'react';
import { Modal } from './Modal.jsx';
import { ListItem } from './ListItem.jsx';

function Foot() {
  return (
    <>
      <button type="button" style={{ padding: '8px 16px', border: 'none', borderRadius: 'var(--r-pill)', background: 'var(--surface-1)', boxShadow: 'var(--e-raised-sm)', color: 'var(--ink-700)', font: '500 13px var(--font-sans)', cursor: 'pointer' }}>Cancel</button>
      <button type="button" style={{ padding: '8px 18px', border: 'none', borderRadius: 'var(--r-pill)', background: 'linear-gradient(180deg, var(--grad-primary-from), var(--grad-primary-to))', boxShadow: 'var(--e-primary-sm)', color: 'var(--on-accent)', font: '600 13px var(--font-sans)', cursor: 'pointer' }}>Save</button>
    </>
  );
}

export default function ModalPreview() {
  return (
    <div className="rux-stage" style={{ display: 'grid', gap: 24, padding: 24 }}>
      <Modal inline title="Prompt Manager" icon="docs" width={720} height={400} boxHeight={480} footer={<Foot />}
        left={(
          <div style={{ padding: 16 }} role="listbox" aria-label="Prompts">
            <ListItem label="Portrait, soft light" pressedWhenSelected selected />
            <ListItem label="Product on marble" pressedWhenSelected />
            <ListItem label="Storyboard frame" pressedWhenSelected />
          </div>
        )}>
        <div style={{ padding: 22 }} className="rux-t-prose">Portrait, soft light: a window on the left, a wool jacket, shallow depth of field.</div>
      </Modal>
      <Modal inline raised title="Confirm" icon="trash" iconTint="var(--danger)" width={420} height={200} boxHeight={320} lightDismiss={false}
        footer={<Foot />}>
        <div style={{ padding: 22 }} className="rux-t-prose">Delete this ticket?</div>
      </Modal>
      <Modal inline present="sheet" title="Choose a model" width={400} height={300} boxHeight={420} footer={<Foot />}>
        <div style={{ padding: 22 }} className="rux-t-prose">claude-sonnet</div>
      </Modal>
    </div>
  );
}

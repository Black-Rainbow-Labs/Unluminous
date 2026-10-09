import React from 'react';
import { Switch } from './Switch.jsx';

export default function SwitchPreview() {
  const row = { display: 'flex', gap: 12, alignItems: 'center' };
  return (
    <div className="rux-stage" style={{ padding: 24, display: 'grid', gap: 16 }}>
      <div style={row}><Switch label="Autosave" /><span className="rux-t-label">Autosave</span></div>
      <div style={row}><Switch defaultOn label="Notifications" /><span className="rux-t-label">Notifications</span></div>
      <div style={row}><Switch disabled label="Sync" /><span className="rux-t-label">Sync (disabled)</span></div>
      <div style={row}><Switch defaultOn disabled label="Backups" /><span className="rux-t-label">Backups (disabled)</span></div>
    </div>
  );
}

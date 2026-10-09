import React from 'react';
import { Avatar } from './Avatar.jsx';

export default function AvatarPreview() {
  return (
    <div className="rux-stage" style={{ padding: 24, display: 'flex', gap: 20, alignItems: 'center' }}>
      <Avatar initials="JM" name="Jason McAffee" />
      <Avatar initials="JM" name="Jason McAffee" status="var(--accent-mint)" />
      <Avatar initials="BR" name="Black Rainbow Labs" status="var(--accent-amber)" diameter={56} />
      <Avatar initials="AB" name="Small account" diameter={28} />
    </div>
  );
}

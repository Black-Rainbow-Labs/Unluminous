import React from 'react';
import { TopNav } from './TopNav.jsx';

const items = [
  { id: 'home', icon: 'home', label: 'Home' },
  { id: 'prompts', icon: 'docs', label: 'Prompts' },
  { id: 'storyboard', icon: 'storyboard', label: 'Storyboard' },
  { id: 'film', icon: 'film', label: 'Film' },
  { id: 'settings', icon: 'settings', label: 'Settings' },
];

export default function TopNavPreview() {
  const [active, setActive] = React.useState(1);
  return (
    <div className="rux-stage" style={{ display: 'grid', gap: 24, justifyItems: 'start', padding: 24 }}>
      <TopNav items={items} active={active} onChange={setActive} />
      <TopNav items={items} active={active} onChange={setActive} fit="icons" />
      <TopNav items={items} active={active} onChange={setActive} fit="trimmed" />
    </div>
  );
}

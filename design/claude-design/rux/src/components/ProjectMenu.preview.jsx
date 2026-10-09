import React from 'react';
import { ProjectMenu } from './ProjectMenu.jsx';

const projects = ['Unluminous', 'Sable', 'Black Rainbow Labs', 'Chordical'];

export default function ProjectMenuPreview() {
  const [selected, setSelected] = React.useState(0);
  return (
    <div className="rux-stage" style={{ display: 'flex', gap: 40, padding: 24, minHeight: 360, alignItems: 'flex-start' }}>
      <ProjectMenu projects={projects} selected={selected} onChoose={setSelected} />
      <ProjectMenu projects={projects} selected={selected} onChoose={setSelected} open />
      <ProjectMenu projects={['A very long project name that is cut off']} selected={0} kicker="Workspace" />
    </div>
  );
}

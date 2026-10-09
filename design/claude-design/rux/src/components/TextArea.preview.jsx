import React from 'react';
import { TextArea } from './TextArea.jsx';

export default function TextAreaPreview() {
  const [prompt, setPrompt] = React.useState('A quiet harbour at dawn, soft light, long shadows on wet stone');
  return (
    <div className="rux-stage" style={{ display: 'grid', gap: 16, gridTemplateColumns: 'repeat(2, 300px)', padding: 24 }}>
      <TextArea label="Prompt" value={prompt} onChange={setPrompt} count corner="wand" hint="Describe the image" />
      <TextArea label="Notes" hint="What needs doing?" style={{ minHeight: 160 }} />
      <TextArea label="Disabled" disabled defaultValue="Locked while rendering" style={{ minHeight: 120 }} />
    </div>
  );
}

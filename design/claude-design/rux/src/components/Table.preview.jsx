import React, { useState } from 'react';
import { Table } from './Table.jsx';
import { Plate, Silk } from './Instrument.jsx';

const columns = [
  { label: 'Crate' },
  { label: 'Debug', align: 'right' },
  { label: 'Release', align: 'right' },
  { label: 'Notes' },
];
const data = [
  ['unluminous-core', '12.4 s', '28.1 s', 'No window; tests run with no fonts'],
  ['unluminous-app', '41.8 s', '96.3 s', 'Fourteen screenshot test binaries share one graphics device'],
  ['unluminous-cli', '6.2 s', '11.0 s', 'Client only, no graphics card'],
  ['unluminous-git', '8.9 s', '17.4 s', 'Runs the machine’s own git'],
];

export default function TablePreview() {
  const [sort, setSort] = useState([2, true]);
  const rows = data
    .slice()
    .sort((a, b) => (parseFloat(a[sort[0]]) - parseFloat(b[sort[0]])) * (sort[1] ? -1 : 1) || 0);
  const press = (at) => {
    if (at === 0 || at === 3) return;
    setSort(sort[0] === at ? [at, !sort[1]] : [at, true]);
  };
  return (
    <div className="rux-stage" style={{ padding: 24, background: 'var(--surface-1)', maxWidth: 620 }}>
      <Plate radius={14} padding={16}>
        <Silk style={{ color: 'var(--ink-400)' }}>Build times</Silk>
        <div style={{ marginTop: 10 }}>
          <Table columns={columns} rows={rows} sorted={sort} onSort={press} />
        </div>
      </Plate>
    </div>
  );
}

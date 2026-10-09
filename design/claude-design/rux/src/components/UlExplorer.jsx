import React from 'react';
import { UlIcon } from './UlIcon.jsx';

const cx = (...parts) => parts.filter(Boolean).join(' ');

/** A small project, the shape the screenshot tests open. */
export const UL_SAMPLE_TREE = [
  { name: 'chapters', folder: true },
  { name: 'drafts', folder: true, open: true },
  { name: 'opening.md', depth: 1, mark: 'prose', git: 'modified' },
  { name: 'bundle.zip', faint: true },
  { name: 'notes.txt', mark: 'page' },
  { name: 'picture.png', mark: 'page' },
  { name: 'program.rs', badge: { letter: 'R', colour: '#b7410e' }, chosen: true },
  { name: 'readme.md', mark: 'prose', git: 'added' },
];

/**
 * The file list down the left of the window (components::explorer): the project's name, the filter
 * box, the tree and the strip counting the files.
 * @param project - the project folder's name, drawn in spaced capitals
 * @param rows - { name, depth, folder, open, chosen, cursor, faint, git, mark, badge, unsaved }
 * @param filter - the words in the filter box
 * @param footer - the counts along the bottom
 * @param keyboard - whether the explorer has the keyboard, which rings the cursor's row
 */
export function UlExplorer({ project = 'unluminous', rows = UL_SAMPLE_TREE, filter = '', footer = '9 files · 8 can be opened · 1 unsaved', keyboard = false, onRow, className, style, ...rest }) {
  return (
    <aside className={cx('ul-explorer', className)} style={style} aria-label="Project" {...rest}>
      <div className="ul-explorer__heading">
        <span className="ul-explorer__project">{project}</span>
        <button type="button" className="ul-explorer__collapse" aria-label="Collapse all folders"><UlIcon name="collapse" size={22} /></button>
      </div>
      <label className="ul-field ul-explorer__filter">
        <UlIcon name="magnifier" size={20} className="ul-field__mark" />
        <input className="ul-field__input" placeholder="Filter files" defaultValue={filter} aria-label="Filter files" />
      </label>
      <ul className="ul-explorer__tree" role="tree">
        {rows.map((row, index) => (
          <li
            key={`${row.name}-${index}`}
            role="treeitem"
            aria-selected={!!row.chosen}
            aria-expanded={row.folder ? !!row.open : undefined}
            className={cx('ul-explorer__row', row.chosen && 'is-chosen', row.cursor && 'is-cursor', row.cursor && keyboard && 'has-keyboard', row.faint && 'is-faint', row.git && `is-git-${row.git}`)}
            style={{ '--depth': row.depth || 0 }}
            onClick={() => onRow && onRow(row, index)}
          >
            <span className="ul-explorer__disclosure">{row.folder && <UlIcon name={row.open ? 'disclosure-open' : 'disclosure-closed'} size={20} />}</span>
            <span className="ul-explorer__mark">{markOf(row)}</span>
            <span className="ul-explorer__name">{row.name}</span>
            {row.unsaved && <span className="ul-explorer__unsaved" aria-label="Unsaved" />}
          </li>
        ))}
      </ul>
      <div className="ul-explorer__footer">{footer}</div>
    </aside>
  );
}

/**
 * The mark in front of a row's name: a folder, a language plugin's badge, or a file mark.
 * @param row - the row
 */
function markOf(row) {
  if (row.folder) return <UlIcon name={row.open ? 'folder-mark-open' : 'folder-mark-closed'} size={20} className={row.open ? 'is-open' : ''} />;
  if (row.badge) return <span className="ul-explorer__badge" style={{ background: row.badge.colour }}>{row.badge.letter}</span>;
  if (row.mark === 'prose') return <UlIcon name="file-mark-prose" size={20} className="is-prose" />;
  if (row.mark === 'code') return <UlIcon name="file-mark-code" size={20} />;
  return <UlIcon name="file-page" size={20} />;
}

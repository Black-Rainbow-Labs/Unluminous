import React from 'react';
import { Icon } from '../icon.jsx';
import { ListItem } from './ListItem.jsx';
import { MenuHeading } from './MenuHeading.jsx';

/**
 * ProjectMenu: the project picker at the top left of the page (Rust `ProjectMenu`, `.proj`).
 * `projects` is a list of names, `selected` an index. `open` is controlled when passed.
 */
export function ProjectMenu({
  projects = [], selected = 0, kicker = 'Project', heading = 'Recent projects', createLabel = 'New project',
  onChoose, onCreate, open: openProp, onToggle, className = '', style, ...rest
}) {
  const [openState, setOpenState] = React.useState(false);
  const controlled = openProp !== undefined;
  const open = controlled ? openProp : openState;
  const root = React.useRef(null);
  const setOpen = (next) => {
    if (!controlled) setOpenState(next);
    if (onToggle) onToggle(next);
  };

  React.useEffect(() => {
    if (!open) return undefined;
    const away = (event) => { if (root.current && !root.current.contains(event.target)) setOpen(false); };
    const key = (event) => { if (event.key === 'Escape') setOpen(false); };
    document.addEventListener('mousedown', away);
    document.addEventListener('keydown', key);
    return () => { document.removeEventListener('mousedown', away); document.removeEventListener('keydown', key); };
  });

  const name = projects[selected] != null ? projects[selected] : '—';
  return (
    <div ref={root} className={('rux-project-menu ' + (open ? 'is-open ' : '') + className).trim()} style={style} {...rest}>
      <button
        type="button" className="rux-project-menu__trigger" aria-haspopup="listbox" aria-expanded={open}
        aria-label={kicker + ': ' + name} onClick={() => setOpen(!open)}
      >
        <span className="rux-project-menu__chip"><Icon name="folder" size={16} /></span>
        <span className="rux-project-menu__body">
          <span className="rux-project-menu__kicker">{kicker}</span>
          <span className="rux-project-menu__name">{name}</span>
        </span>
        <span className="rux-project-menu__chev"><Icon name="chevDown" size={15} /></span>
      </button>
      {open && (
        <div className="rux-project-menu__menu">
          <MenuHeading>{heading}</MenuHeading>
          <div role="listbox" aria-label={heading}>
            {projects.map((project, index) => (
              <ListItem
                key={index} label={project} dot selected={index === selected}
                onClick={() => { if (onChoose) onChoose(index); setOpen(false); }}
              />
            ))}
          </div>
          <div className="rux-project-menu__foot">
            <button
              type="button" className="rux-project-menu__new"
              onClick={() => { if (onCreate) onCreate(); setOpen(false); }}
            >
              <Icon name="plus" size={14} />
              <span>{createLabel}</span>
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

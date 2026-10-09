// The rux icon set: the 37 SbIcon marks, drawn from the path data rux itself keeps.
//
// icons.data.json is written by tools/extract-rux-icons.mjs from crates/rux/src/icon/paths.rs, so a
// mark here is the mark the Rust draws. The parent <svg> strokes in currentColor at 1.8 on a 24 unit
// viewBox with round caps and joins; a child marked fill also keeps that stroke, and only play and
// pause turn it off, which is what SbIcon does.

import icons from './icons.data.json';

/** Every icon name, in SbIcon's own order. */
export const ICON_NAMES = Object.keys(icons);

/**
 * Draw one rux icon in the current text colour.
 * @param name - the SbIcon name, e.g. "search" or "chevDown"
 * @param size - the side of the square in px
 * @param stroke - the stroke width in viewBox units
 */
export function Icon({ name, size = 16, stroke = 1.8, className = '', style, ...rest }) {
  const parts = icons[name];
  if (!parts) return null;
  return (
    <svg
      className={`rux-icon ${className}`.trim()}
      style={style}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={stroke}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden={rest['aria-label'] ? undefined : true}
      focusable="false"
      {...rest}
    >
      {parts.map((part, index) => drawPart(part, index))}
    </svg>
  );
}

/**
 * One child of the icon's <svg>.
 * @param part - a path, circle or rect record
 * @param index - its position, used as the React key
 */
function drawPart(part, index) {
  const paint = { fill: part.fill ? 'currentColor' : undefined, stroke: part.stroke ? undefined : 'none' };
  if (part.kind === 'path') return <path key={index} d={part.d} {...paint} />;
  if (part.kind === 'circle') return <circle key={index} cx={part.cx} cy={part.cy} r={part.r} {...paint} />;
  return <rect key={index} x={part.x} y={part.y} width={part.w} height={part.h} rx={part.rx} {...paint} />;
}

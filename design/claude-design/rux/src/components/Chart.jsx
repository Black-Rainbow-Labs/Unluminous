import React, { useEffect, useId, useMemo, useRef, useState } from 'react';
import { alpha, brighten, easeBack, easeOut, Led, progress, Screen, seriesColor, Silk, TIMING, useClock } from './Instrument.jsx';

const AXIS_CHAR = 9.5 * 0.6; // JetBrains Mono advance at 9.5px

/**
 * Round numbers to put gridlines at between `low` and `high`: about `count` of them, each 1, 2, 2.5 or 5
 * times a power of ten apart, always covering the data. Rust `chart::nice_ticks`.
 */
export function niceTicks(low, high, count) {
  let lo;
  let hi;
  if (Number.isFinite(low) && Number.isFinite(high)) {
    if (high > low) {
      lo = low;
      hi = high;
    } else {
      lo = Math.min(low, 0);
      hi = Math.max(low, 0) + 1;
    }
  } else {
    lo = 0;
    hi = 1;
  }
  const rough = (hi - lo) / Math.max(count, 1);
  const power = Math.pow(10, Math.floor(Math.log10(rough)));
  const step = [1, 2, 2.5, 5, 10].map((f) => f * power).find((s) => s >= rough) ?? power * 10;
  const out = [];
  let at = Math.floor(lo / step) * step;
  while (at < hi + step * 0.999 && out.length < 50) {
    out.push(Math.round(at / step) * step);
    at += step;
  }
  return out;
}

function pointCount(labels, series) {
  return Math.max(labels.length, ...series.map((s) => s.values.length), 0);
}

function valueRange(labels, series, stacked) {
  let low = 0;
  let high = 0;
  const points = pointCount(labels, series);
  for (let i = 0; i < points; i += 1) {
    const values = series.map((s) => s.values[i]).filter((v) => v !== undefined);
    if (stacked) {
      let up = 0;
      let down = 0;
      values.forEach((v) => (v >= 0 ? (up += v) : (down += v)));
      high = Math.max(high, up);
      low = Math.min(low, down);
    } else {
      values.forEach((v) => {
        high = Math.max(high, v);
        low = Math.min(low, v);
      });
    }
  }
  return [low, high];
}

/** Legend rows wrapped across `width`: 16px a row, each entry 14 + words + 16. */
function legendRows(series, width) {
  let rows = 1;
  let pen = 0;
  series.forEach((one) => {
    const need = 14 + one.name.length * 11 * 0.55 + 16;
    if (pen > 0 && pen + need > width) {
      rows += 1;
      pen = 0;
    }
    pen += need;
  });
  return rows;
}

function elide(text, room) {
  const fit = Math.floor(room / AXIS_CHAR);
  return text.length <= fit ? text : `${text.slice(0, Math.max(0, fit - 1))}…`;
}

/** A filled band of a ring between two angles (Rust `ring_arc`). */
function arcPath(cx, cy, inner, outer, from, to) {
  const p = (r, a) => `${cx + r * Math.cos(a)} ${cy + r * Math.sin(a)}`;
  const large = to - from > Math.PI ? 1 : 0;
  return `M ${p(outer, from)} A ${outer} ${outer} 0 ${large} 1 ${p(outer, to)} L ${p(inner, to)} A ${inner} ${inner} 0 ${large} 0 ${p(inner, from)} Z`;
}

function useWidth(fallback) {
  const ref = useRef(null);
  const [width, setWidth] = useState(fallback);
  useEffect(() => {
    if (!ref.current || typeof ResizeObserver === 'undefined') return undefined;
    const watch = new ResizeObserver(([entry]) => setWidth(entry.contentRect.width));
    watch.observe(ref.current);
    setWidth(ref.current.getBoundingClientRect().width);
    return () => watch.disconnect();
  }, []);
  return [ref, width];
}

/**
 * A chart on a screen: bars made of lit segments (`kind="bar"`), a glowing trace (`"line"`), an area under
 * one (`"area"`), or a segmented ring (`"donut"`). Rust `chart::Chart`.
 *
 * `series` is `[{ name, values, colour }]` (colour defaults to the encoder colours in order). `labels` are
 * the categories. `format(value)` writes the numbers on the axes. The value under the pointer is read out
 * in the screen's top right corner. `onHover({ series, index } | null)` reports it.
 */
export function Chart({
  kind = 'bar',
  labels = [],
  series = [],
  stacked = false,
  format = (v) => String(v),
  animate = true,
  onHover,
  className = '',
  style,
  ...rest
}) {
  const [ref, width] = useWidth(480);
  const full = useMemo(
    () => series.map((s, i) => ({ ...s, colour: s.colour ?? seriesColor(i) })),
    [series],
  );
  const describe = `Chart: ${full.map((s) => s.name).join(', ')} over ${labels.join(', ')}`;
  return (
    <div ref={ref} role="img" aria-label={describe} className={`rux-chart ${className}`.trim()} style={style} {...rest}>
      {kind === 'donut' ? (
        <Donut width={width} labels={labels} series={full} animate={animate} onHover={onHover} />
      ) : (
        <Plot
          kind={kind}
          width={width}
          labels={labels}
          series={full}
          stacked={stacked}
          format={format}
          animate={animate}
          onHover={onHover}
        />
      )}
    </div>
  );
}

function Plot({ kind, width, labels, series, stacked, format, animate, onHover }) {
  const clipId = `rux-chart-clip-${useId().replace(/:/g, '')}`;
  const [pointer, setPointer] = useState(null);
  const points = pointCount(labels, series);
  const legend = series.length > 1;
  const legendHeight = legend ? legendRows(series, width) * 16 : 0;
  const H = 160;
  const [low, high] = valueRange(labels, series, stacked);
  const ticks = niceTicks(low, high, 5);
  const bottomValue = ticks[0] ?? 0;
  const topValue = ticks[ticks.length - 1] ?? 1;
  const widest = Math.max(0, ...ticks.map((t) => format(t).length * AXIS_CHAR));
  const plot = { left: widest + 14, top: 22, right: width - 12, bottom: H - 22 };
  plot.width = plot.right - plot.left;
  plot.height = plot.bottom - plot.top;
  const total = kind === 'bar' ? points * series.length * TIMING.STAGGER + TIMING.RISE : series.length * TIMING.STAGGER * 3 + TIMING.RISE * 1.6;
  const t = useClock(animate, total + 0.1);

  const ok = plot.width >= 20 && plot.height >= 20 && points > 0;
  const group = ok ? plot.width / points : 1;
  const yOf = (v) => plot.bottom - ((v - bottomValue) / Math.max(topValue - bottomValue, Number.EPSILON)) * plot.height;
  const zero = yOf(Math.min(topValue, Math.max(bottomValue, 0)));

  // Which point is under the pointer.
  let hovered = null;
  if (ok && pointer) {
    if (kind === 'bar') {
      const lanes = stacked ? 1 : Math.max(series.length, 1);
      const gap = 4;
      const bw = Math.min(16, Math.max(3, (group * 0.72 - gap * (lanes - 1)) / lanes));
      const tot = bw * lanes + gap * (lanes - 1);
      for (let index = 0; index < points; index += 1) {
        const start = plot.left + group * (index + 0.5) - tot / 2;
        series.forEach((one, s) => {
          if (one.values[index] === undefined) return;
          const left = start + (stacked ? 0 : s) * (bw + gap);
          if (pointer.x >= left - gap / 2 && pointer.x <= left + bw + gap / 2 && pointer.y >= plot.top && pointer.y <= plot.bottom + 18) {
            hovered = { series: s, index };
          }
        });
      }
    } else if (pointer.x >= plot.left - 10 && pointer.x <= plot.right + 10 && pointer.y >= plot.top - 10 && pointer.y <= plot.bottom + 10) {
      const index = Math.min(points - 1, Math.max(0, Math.floor((pointer.x - plot.left) / group)));
      hovered = { series: series.length - 1, index };
    }
  }
  useEffect(() => {
    if (onHover) onHover(hovered);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hovered && hovered.series, hovered && hovered.index]);

  // Category labels: every nth when they do not fit a column each, always counting from the first.
  const widest_label = Math.max(0, ...labels.map((l) => l.length * AXIS_CHAR));
  let stride = 1;
  if (!(widest_label + 6 <= group)) {
    const need = Math.ceil((widest_label + 6) / Math.max(group, 1));
    stride = [2, 3, 4, 5, 10, 20, 25, 50].find((s) => s >= need) ?? need;
  }

  const onMove = (event) => {
    const box = event.currentTarget.getBoundingClientRect();
    setPointer({ x: event.clientX - box.left, y: event.clientY - box.top });
  };

  const body = [];
  if (ok && kind === 'bar') {
    const lanes = stacked ? 1 : Math.max(series.length, 1);
    const gap = 4;
    const bw = Math.min(16, Math.max(3, (group * 0.72 - gap * (lanes - 1)) / lanes));
    const tot = bw * lanes + gap * (lanes - 1);
    let drawn = 0;
    for (let index = 0; index < points; index += 1) {
      const start = plot.left + group * (index + 0.5) - tot / 2;
      let up = 0;
      let down = 0;
      series.forEach((one, s) => {
        const value = one.values[index];
        if (value === undefined) return;
        let from = 0;
        if (stacked) {
          from = value >= 0 ? up : down;
          if (value >= 0) up += value;
          else down += value;
        }
        const to = from + value;
        const left = start + (stacked ? 0 : s) * (bw + gap);
        const rise = easeBack(progress(t, drawn * TIMING.STAGGER, TIMING.RISE));
        drawn += 1;
        const yFrom = yOf(from);
        const yTo = yOf(from + (to - from) * rise);
        const top = Math.min(yFrom, yTo);
        const bottom = Math.max(yFrom, yTo);
        const isHover = hovered && hovered.series === s && hovered.index === index;
        const colour = isHover ? brighten(one.colour) : one.colour;
        const upward = to >= from;
        const segs = [];
        let edge = upward ? bottom : top;
        const end = upward ? top : bottom;
        while ((upward && edge > end + 0.5) || (!upward && edge < end - 0.5)) {
          const next = upward ? Math.max(edge - 5, end) : Math.min(edge + 5, end);
          segs.push(<rect key={segs.length} x={left} y={Math.min(edge, next)} width={bw} height={Math.abs(next - edge)} rx="1" fill={colour} />);
          edge = upward ? next - 2 : next + 2;
        }
        body.push(
          <g key={`${index}-${s}`}>
            <rect x={left - 2} y={top - 2} width={bw + 4} height={bottom - top + 4} rx="3" fill={alpha(colour, 0.1)} />
            {segs}
          </g>,
        );
      });
    }
  }

  if (ok && kind !== 'bar') {
    const below = new Array(points).fill(0);
    series.forEach((one, s) => {
      const draw = easeOut(progress(t, s * TIMING.STAGGER * 3, TIMING.RISE * 1.6));
      const pts = one.values.map((v, i) => ({ x: plot.left + group * (i + 0.5), y: yOf((stacked ? below[i] : 0) + v) }));
      const bases = pts.map((_, i) => (stacked ? yOf(below[i]) : zero));
      if (stacked) one.values.forEach((v, i) => (below[i] += v));
      if (!pts.length) return;
      const reach = plot.left + plot.width * draw;
      const id = `${clipId}-${s}`;
      const line = pts.map((p) => `${p.x},${p.y}`).join(' ');
      body.push(
        <g key={`trace-${s}`}>
          <clipPath id={id}>
            <rect x={plot.left - 20} y="0" width={Math.max(0, reach - plot.left + 20)} height={H} />
          </clipPath>
          <g clipPath={`url(#${id})`}>
            {kind === 'area'
              ? pts.slice(0, -1).map((p, i) => {
                  const q = pts[i + 1];
                  const gid = `${id}-g${i}`;
                  const topY = Math.min(p.y, q.y);
                  const baseY = Math.max(bases[i], bases[i + 1]);
                  return (
                    <g key={i}>
                      <linearGradient id={gid} gradientUnits="userSpaceOnUse" x1="0" y1={topY} x2="0" y2={baseY}>
                        <stop offset="0" stopColor={one.colour} stopOpacity="0.30" />
                        <stop offset="1" stopColor={one.colour} stopOpacity="0.02" />
                      </linearGradient>
                      <polygon points={`${p.x},${p.y} ${q.x},${q.y} ${q.x},${bases[i + 1]} ${p.x},${bases[i]}`} fill={`url(#${gid})`} />
                    </g>
                  );
                })
              : null}
            {pts.length > 1 ? (
              <>
                <polyline points={line} fill="none" stroke={one.colour} strokeOpacity="0.08" strokeWidth="6" strokeLinejoin="round" strokeLinecap="round" />
                <polyline points={line} fill="none" stroke={one.colour} strokeOpacity="0.18" strokeWidth="3" strokeLinejoin="round" strokeLinecap="round" />
                <polyline points={line} fill="none" stroke={one.colour} strokeWidth="1.6" strokeLinejoin="round" strokeLinecap="round" />
              </>
            ) : null}
          </g>
          {pts.map((p, i) => {
            if (p.x > reach + 0.5) return null;
            const lit = hovered && hovered.index === i;
            const r = lit ? 3.5 : 2.4;
            return (
              <g key={i}>
                <circle cx={p.x} cy={p.y} r={r + 1.2} fill="var(--inst-screen)" />
                <circle cx={p.x} cy={p.y} r={r} fill={lit ? brighten(one.colour) : one.colour} />
              </g>
            );
          })}
        </g>,
      );
    });
  }

  const readout = hovered && series[hovered.series];
  const readWords = readout
    ? `${labels[hovered.index] ?? ''}${series.length > 1 ? ` · ${readout.name} ` : ' '}${format(readout.values[hovered.index] ?? 0)}`
    : '';

  return (
    <>
      <Screen radius={9} graticule className="rux-chart__screen" style={{ height: H }}>
        <svg
          className="rux-chart__svg"
          width={width}
          height={H}
          viewBox={`0 0 ${width} ${H}`}
          onPointerMove={onMove}
          onPointerLeave={() => setPointer(null)}
        >
          {ticks.map((tick) => {
            const y = yOf(tick);
            return (
              <g key={tick}>
                <line x1={plot.left} x2={plot.right} y1={Math.round(y) + 0.5} y2={Math.round(y) + 0.5} stroke="var(--inst-rule)" strokeDasharray="2 4" />
                <text className="rux-chart__axis" x={plot.left - 8} y={y} textAnchor="end" dominantBaseline="central" fill="var(--ink-300)">
                  {format(tick)}
                </text>
              </g>
            );
          })}
          {labels.map((label, index) =>
            index % stride ? null : (
              <text
                key={index}
                className="rux-chart__axis"
                x={Math.max(plot.left - 6 + (elide(label, group * stride - 4).length * AXIS_CHAR) / 2, plot.left + group * (index + 0.5))}
                y={plot.bottom + 6}
                textAnchor="middle"
                dominantBaseline="hanging"
                fill="var(--ink-400)"
              >
                {elide(label, group * stride - 4)}
              </text>
            ),
          )}
          {body}
          {hovered && kind !== 'bar' && ok ? (
            <line
              x1={Math.round(plot.left + group * (hovered.index + 0.5)) + 0.5}
              x2={Math.round(plot.left + group * (hovered.index + 0.5)) + 0.5}
              y1={plot.top}
              y2={plot.bottom}
              stroke="var(--inst-rule-strong)"
              strokeDasharray="2 3"
            />
          ) : null}
        </svg>
        {readout ? (
          <div className="rux-chart__readout">
            <Led colour={readout.colour} radius={2.5} />
            <span>{readWords}</span>
          </div>
        ) : null}
      </Screen>
      {legend ? (
        <div className="rux-chart__legend">
          {series.map((one) => (
            <span key={one.name} className="rux-chart__legend-item">
              <Led colour={one.colour} radius={3} />
              {one.name}
            </span>
          ))}
        </div>
      ) : null}
    </>
  );
}

function Donut({ width, labels, series, animate, onHover }) {
  const [pointer, setPointer] = useState(null);
  const H = Math.max(Math.min(140, width * 0.6), 110);
  const side = Math.min(H, width * 0.5);
  const centre = side / 2;
  const outer = side / 2 - 9;
  const inner = outer * 0.66;
  const values = (series[0] ? series[0].values : []).map((v) => Math.max(0, v));
  const total = values.reduce((a, b) => a + b, 0);
  const t = useClock(animate, TIMING.RISE * 1.8 + 0.1);
  const sweep = easeOut(progress(t, 0, TIMING.RISE * 1.8));
  const gap = (1.6 * Math.PI) / 180;
  let angle = -Math.PI / 2;
  let hovered = null;
  const slices = [];
  const colourOf = (i) => seriesColor(i);
  if (total > 0) {
    values.forEach((value, index) => {
      const share = (value / total) * Math.PI * 2 * sweep;
      if (share <= gap * 2) {
        angle += share;
        return;
      }
      const from = angle + gap;
      const to = angle + share - gap;
      let isHover = false;
      if (pointer) {
        const dx = pointer.x - centre;
        const dy = pointer.y - centre;
        const r = Math.hypot(dx, dy);
        let a = Math.atan2(dy, dx);
        while (a < -Math.PI / 2) a += Math.PI * 2;
        isHover = r >= inner && r <= outer + 4 && a >= from && a <= to;
      }
      if (isHover) hovered = { series: 0, index };
      const colour = isHover ? brighten(colourOf(index)) : colourOf(index);
      const reach = isHover ? 3 : 0;
      slices.push(
        <g key={index}>
          <path d={arcPath(centre, centre, inner - 2, outer + 2 + reach, from, to)} fill={alpha(colour, 0.1)} />
          <path d={arcPath(centre, centre, inner, outer + reach, from, to)} fill={colour} />
        </g>,
      );
      angle += share;
    });
  }
  useEffect(() => {
    if (onHover) onHover(hovered);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hovered && hovered.index]);

  let shown = hovered ? hovered.index : null;
  if (shown === null && values.length) shown = values.reduce((best, v, i) => (v > values[best] ? i : best), 0);
  const rowH = 18;
  const rowsTop = side / 2 - (rowH * values.length) / 2;

  return (
    <div className="rux-chart__donut" style={{ height: H }}>
      <Screen radius={side / 2} className="rux-chart__dial" style={{ width: side, height: side }}>
        <svg
          width={side}
          height={side}
          viewBox={`0 0 ${side} ${side}`}
          onPointerMove={(e) => {
            const box = e.currentTarget.getBoundingClientRect();
            setPointer({ x: e.clientX - box.left, y: e.clientY - box.top });
          }}
          onPointerLeave={() => setPointer(null)}
        >
          {slices}
        </svg>
        {total > 0 && shown !== null ? (
          <div className="rux-chart__centre" style={{ maxWidth: inner * 1.6 }}>
            <span className="rux-chart__percent">{Math.round((values[shown] / total) * 100)}%</span>
            <Silk className="rux-chart__centre-label">{labels[shown] ?? ''}</Silk>
          </div>
        ) : null}
      </Screen>
      <div className="rux-chart__donut-legend" style={{ left: side + 18, top: rowsTop }}>
        {labels.slice(0, values.length).map((label, index) => {
          const lit = hovered && hovered.index === index;
          return (
            <div key={index} className={`rux-chart__row${lit ? ' is-lit' : ''}`} style={{ height: rowH }}>
              <Led colour={colourOf(index)} radius={3} />
              <span className="rux-chart__row-name">{label}</span>
              <span className="rux-chart__row-share">{total > 0 ? `${Math.round((values[index] / total) * 100)}%` : ''}</span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

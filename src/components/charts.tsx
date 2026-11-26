import { useState } from "react";
import { Bar, BarChart, CartesianGrid, Legend, Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { fmtCompact, fmtNumber } from "../lib/format";
import { useT } from "../lib/i18n";

export const SERIES_COLORS = ["#7c5cff", "#2dd4bf", "#ff5d8f", "#f59e0b", "#38bdf8", "#a3e635", "#f472b6", "#94a3b8"];
export const PLATFORM_HEX = { instagram: "#e1306c", facebook: "#1877f2", youtube: "#ff3b30", spotify: "#1db954" } as const;

export interface Series { key: string; label: string; color?: string }

/** Pivot long rows into chart rows keyed by day. Missing days stay missing (gaps), never zero. */
export function pivot<T extends { day: string; value: number }>(rows: T[], keyOf: (r: T) => string): Record<string, number | string>[] {
  const by = new Map<string, Record<string, number | string>>();
  for (const r of rows) {
    const row = by.get(r.day) ?? { day: r.day };
    row[keyOf(r)] = r.value;
    by.set(r.day, row);
  }
  return [...by.values()].sort((a, b) => String(a.day).localeCompare(String(b.day)));
}

const axis = { stroke: "var(--muted)", fontSize: 11, tickLine: false, axisLine: false } as const;

function TooltipBox({ active, payload, label }: { active?: boolean; payload?: { name: string; value: number; color: string }[]; label?: string }) {
  const t = useT();
  if (!active || !payload?.length) return null;
  return (
    <div className="rounded-lg border border-[var(--line)] bg-[var(--panel)] px-3 py-2 text-xs shadow-lg">
      <div className="mb-1 font-medium">{label}</div>
      {payload.map((p) => (
        <div key={p.name} className="flex items-center gap-2"><span className="size-2 rounded-full" style={{ background: p.color }} />{p.name}: <span className="num">{fmtNumber(p.value, t.lang)}</span></div>
      ))}
    </div>
  );
}

export function TrendChart({ data, series, height = 240, zeroBased = true }: { data: Record<string, number | string>[]; series: Series[]; height?: number; zeroBased?: boolean }) {
  const t = useT();
  return (
    <div style={{ height }} role="img" aria-label={series.map((s) => s.label).join(", ")}>
      <ResponsiveContainer width="100%" height="100%">
        <LineChart data={data} margin={{ top: 8, right: 8, bottom: 0, left: 0 }}>
          <CartesianGrid stroke="var(--line)" strokeDasharray="3 3" vertical={false} />
          <XAxis dataKey="day" {...axis} minTickGap={24} />
          <YAxis {...axis} width={44} domain={zeroBased ? [0, "auto"] : ["auto", "auto"]} tickFormatter={(v) => fmtCompact(v, t.lang)} />
          <Tooltip content={<TooltipBox />} />
          {series.length > 1 && <Legend wrapperStyle={{ fontSize: 12 }} />}
          {series.map((s, i) => (
            <Line key={s.key} type="monotone" dataKey={s.key} name={s.label} stroke={s.color ?? SERIES_COLORS[i % SERIES_COLORS.length]} strokeWidth={2} dot={false} connectNulls={false} isAnimationActive={false} />
          ))}
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}

export function Bars({ data, series, xKey = "name", height = 240, horizontal }: { data: Record<string, number | string>[]; series: Series[]; xKey?: string; height?: number; horizontal?: boolean }) {
  const t = useT();
  return (
    <div style={{ height }} role="img" aria-label={series.map((s) => s.label).join(", ")}>
      <ResponsiveContainer width="100%" height="100%">
        <BarChart data={data} layout={horizontal ? "vertical" : "horizontal"} margin={{ top: 8, right: 8, bottom: 0, left: 0 }}>
          <CartesianGrid stroke="var(--line)" strokeDasharray="3 3" vertical={!horizontal} horizontal={!horizontal} />
          {horizontal ? (
            <>
              <XAxis type="number" {...axis} tickFormatter={(v) => fmtCompact(v, t.lang)} />
              <YAxis type="category" dataKey={xKey} {...axis} width={140} />
            </>
          ) : (
            <>
              <XAxis dataKey={xKey} {...axis} />
              <YAxis {...axis} width={44} tickFormatter={(v) => fmtCompact(v, t.lang)} />
            </>
          )}
          <Tooltip content={<TooltipBox />} cursor={{ fill: "var(--panel-2)" }} />
          {series.length > 1 && <Legend wrapperStyle={{ fontSize: 12 }} />}
          {series.map((s, i) => <Bar key={s.key} dataKey={s.key} name={s.label} fill={s.color ?? SERIES_COLORS[i % SERIES_COLORS.length]} radius={3} isAnimationActive={false} />)}
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}

/** One metric at a time: metrics on different scales never share an axis. */
export function MetricTrend({ rows, metrics, label, color, height = 220, zeroBased }: { rows: { day: string; metric: string; value: number }[]; metrics: string[]; label: (m: string) => string; color?: string | ((m: string) => string); height?: number; zeroBased?: boolean }) {
  const [sel, setSel] = useState(metrics[0]);
  const m = metrics.includes(sel) ? sel : metrics[0];
  return (
    <div>
      {metrics.length > 1 && (
        <div className="mb-2 flex flex-wrap gap-1" role="group">
          {metrics.map((x) => (
            <button key={x} type="button" aria-pressed={x === m} onClick={() => setSel(x)}
              className={`rounded-full border px-2.5 py-0.5 text-xs cursor-pointer ${x === m ? "border-iris bg-iris/15 text-[var(--ink)]" : "border-[var(--line)] text-[var(--muted)] hover:text-[var(--ink)]"}`}>{label(x)}</button>
          ))}
        </div>
      )}
      <TrendChart data={pivot(rows.filter((r) => r.metric === m), (r) => r.metric)} series={[{ key: m, label: label(m), color: typeof color === "function" ? color(m) : color }]} height={height} zeroBased={zeroBased} />
    </div>
  );
}

<script>
export let series = { rust: [], cpp: [] };
export let metric = "";
export let unit = "";
export let tr = (key) => key;

$: all = [...series.rust, ...series.cpp];
$: dates = [...new Set(all.map((point) => point.date))].sort((a, b) => a - b);
$: min = all.length ? Math.min(...all.map((point) => point.value)) : 0;
$: max = all.length ? Math.max(...all.map((point) => point.value)) : 1;
$: x = (date) =>
  48 + (dates.indexOf(date) * 704) / Math.max(1, dates.length - 1);
$: y = (value) => 216 - ((value - min) / (max - min || 1)) * 168;
$: path = (points) =>
  points.map((point, i) => `${i ? "L" : "M"}${x(point.date)},${y(point.value)}`).join(" ");

const fmt = (value) => (Math.abs(value) >= 1000 ? value.toFixed(0) : value.toFixed(2));
</script>

{#if all.length}
  <svg viewBox="0 0 800 260" role="img" aria-label={metric}>
    {#each [0, 0.25, 0.5, 0.75, 1] as tick}
      <line x1="48" x2="752" y1={216 - tick * 168} y2={216 - tick * 168} class="GridLine" />
      <text x="40" y={216 - tick * 168 + 4} text-anchor="end" class="Axis">
        {fmt(min + (max - min) * tick)}
      </text>
    {/each}
    {#each dates as date}
      <text x={x(date)} y="244" text-anchor="middle" class="Axis">
        {new Date(date * 1000).toISOString().slice(5, 10)}
      </text>
    {/each}
    {#if series.rust.length}<path d={path(series.rust)} class="Line Rust" />{/if}
    {#if series.cpp.length}<path d={path(series.cpp)} class="Line Cpp" />{/if}
    {#each series.rust as point}
      <circle cx={x(point.date)} cy={y(point.value)} r="3" class="Dot Rust" />
    {/each}
    {#each series.cpp as point}
      <circle cx={x(point.date)} cy={y(point.value)} r="3" class="Dot Cpp" />
    {/each}
  </svg>
  <div class="Legend">
    <span class="Rust">Rust</span>
    <span class="Cpp">C++</span>
    <span>{metric} · {unit}</span>
  </div>
{:else}
  <p class="Empty">{tr("noData")}</p>
{/if}

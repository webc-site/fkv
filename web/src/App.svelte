<script>
import { onMount } from "svelte";

import { LANG_LI, detectLang, t } from "./i18n.js";
import PerfChart from "./PerfChart.svelte";

let lang = detectLang(),
  data = { runs: [], baseline: "", generated_at: 0 },
  metric = "",
  platform = "";

$: localStorage.setItem("fkv_lang", lang);
$: tr = (key) => t(lang, key);
$: runs = data.runs || [];
$: metric_li = [...new Set(runs.map((run) => run.metric))];
$: platform_li = [...new Set(runs.map((run) => run.platform))];
$: if (!metric && metric_li.length) metric = metric_li[0];
$: if (!platform && platform_li.length) platform = platform_li[0];
$: filtered = runs
  .filter((run) => run.metric === metric && run.platform === platform)
  .sort((a, b) => a.date - b.date);
$: series = {
  rust: filtered.filter((run) => run.impl === "rust"),
  cpp: filtered.filter((run) => run.impl === "cpp"),
};
$: latest = (impl_name) => {
  const list = filtered.filter((run) => run.impl === impl_name);
  return list.length ? list[list.length - 1] : null;
};
$: rust_latest = latest("rust");
$: cpp_latest = latest("cpp");
$: ratio =
  rust_latest && cpp_latest && cpp_latest.value
    ? rust_latest.value / cpp_latest.value
    : null;
$: unit = filtered.length ? filtered[0].unit : "";

const fmt = (value) => {
  if (value === null || value === undefined) return "—";
  if (Math.abs(value) >= 1000) return value.toFixed(0);
  return value.toFixed(2);
};

const day = (ts) => new Date(ts * 1000).toISOString().slice(0, 10);

onMount(async () => {
  try {
    const res = await fetch("/perf.json", { cache: "no-store" });
    if (res.ok) data = await res.json();
  } catch (err) {
    console.error(err);
  }
});
</script>

<svelte:head><title>fkv · performance</title></svelte:head>

<main class="Wrap">
  <header class="Top">
    <div>
      <h1>{tr("title")}</h1>
      <p>{tr("subtitle")}</p>
    </div>
    <div class="Lang">
      {#each LANG_LI as l}
        <button class="Btn" class:Main={l === lang} on:click={() => (lang = l)}>{l}</button>
      {/each}
    </div>
  </header>

  <section class="Meta">
    <span class="Badge">{tr("baseline")} {data.baseline || "—"}</span>
    <span class="Badge">{tr("updated")} {data.generated_at ? day(data.generated_at) : "—"}</span>
  </section>

  {#if !filtered.length}
    <p class="Empty">{tr("noData")}</p>
  {:else}
    <section class="Grid">
      <div class="Card">
        <h2>{tr("latestRust")}</h2>
        <b>{fmt(rust_latest?.value)} {unit}</b>
        <i>{rust_latest ? rust_latest.commit.slice(0, 8) : "—"}</i>
      </div>
      <div class="Card">
        <h2>{tr("latestCpp")}</h2>
        <b>{fmt(cpp_latest?.value)} {unit}</b>
        <i>{cpp_latest ? cpp_latest.commit.slice(0, 8) : "—"}</i>
      </div>
      <div class="Card">
        <h2>{tr("ratio")}</h2>
        <b>{ratio ? ratio.toFixed(2) : "—"}</b>
        <i>Rust / C++</i>
      </div>
      <div class="Card">
        <h2>{tr("runs")}</h2>
        <b>{filtered.length}</b>
        <i>{tr("history")}</i>
      </div>
    </section>

    <section class="Panel">
      <div class="PanelHead">
        <h2>{tr("chart")}</h2>
        <div class="Selects">
          <label>{tr("metric")}
            <select bind:value={metric}>
              {#each metric_li as m}<option value={m}>{m}</option>{/each}
            </select>
          </label>
          <label>{tr("platform")}
            <select bind:value={platform}>
              {#each platform_li as p}<option value={p}>{p}</option>{/each}
            </select>
          </label>
        </div>
      </div>
      <PerfChart {series} {metric} {unit} {tr} />
    </section>

    <section class="Panel">
      <h2>{tr("history")}</h2>
      <table>
        <thead>
          <tr>
            <th>{tr("date")}</th>
            <th>{tr("commit")}</th>
            <th>{tr("impl")}</th>
            <th>{tr("value")}</th>
          </tr>
        </thead>
        <tbody>
          {#each [...filtered].reverse().slice(0, 30) as run}
            <tr>
              <td>{day(run.date)}</td>
              <td><code>{run.commit.slice(0, 8)}</code></td>
              <td>{run.impl}</td>
              <td>{fmt(run.value)} {run.unit}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </section>
  {/if}
</main>

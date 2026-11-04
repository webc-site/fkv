const DICT = {
  zh: {
    title: "fkv 性能看板",
    subtitle: "Rust 版 eloqstore · 每次提交的性能曲线与 C++ 对比",
    latestRust: "最新 Rust",
    latestCpp: "最新 C++",
    ratio: "相对比值",
    runs: "运行数",
    metric: "指标",
    platform: "平台",
    history: "历史",
    date: "日期",
    commit: "提交",
    impl: "实现",
    value: "数值",
    noData: "还没有数据，先跑一次 perf workflow。",
    updated: "更新于",
    baseline: "兼容基线",
    chart: "趋势",
  },
  en: {
    title: "fkv performance",
    subtitle: "Rust port of eloqstore · per-commit curves and C++ comparison",
    latestRust: "Latest Rust",
    latestCpp: "Latest C++",
    ratio: "Ratio",
    runs: "Runs",
    metric: "Metric",
    platform: "Platform",
    history: "History",
    date: "Date",
    commit: "Commit",
    impl: "Impl",
    value: "Value",
    noData: "No data yet. Run the perf workflow first.",
    updated: "Updated",
    baseline: "Baseline",
    chart: "Trend",
  },
};

export const LANG_LI = ["zh", "en"];

export function detectLang() {
  const from_query = new URLSearchParams(location.search).get("lang");
  if (from_query && DICT[from_query]) return from_query;
  const from_store = localStorage.getItem("fkv_lang");
  if (from_store && DICT[from_store]) return from_store;
  return navigator.language.startsWith("zh") ? "zh" : "en";
}

export function t(lang, key) {
  return DICT[lang]?.[key] ?? DICT.en[key] ?? key;
}

import { newEl } from "../../../js/dom.js";
import headerNew from "../../../js/dom/header.js";
import "../../H1Txt.js";
import "../../Card.js";
import {
  DOC_LINKS,
  PERF_CONFIG,
  PERF_MATRIX_ROWS,
  PERF_META,
  PERF_NOTE,
  VERIFY_ROWS
} from "../../../conf/perf.js";

const GITHUB = "https://github.com/webc-site/fkv",
  section = (title) => {
    const sec = newEl("section"),
      h2 = newEl("h2");
    sec.className = "HomeSection";
    h2.className = "O";
    h2.textContent = title;
    sec.append(h2);
    return sec;
  },
  cell = (tag, text, cls) => {
    const el = newEl(tag);
    el.textContent = text;
    if (cls) el.className = cls;
    return el;
  };

let res_li;

const page = () => {
  const el = newEl("main"),
    [header, h1, brief] = headerNew(),
    [badge, sub] = [newEl("span"), newEl("p")];

  el.className = "Home";

  header.className += " HomeHero";
  badge.className = "HomeBadge";
  badge.textContent = "Rust · compio · 盘面页式 KV";
  h1.textContent = "fkv";
  brief.textContent = "纯 Rust KV 引擎：格式对齐 C++ 语义，全链路自研 IO";
  sub.className = "HomeSub";
  sub.textContent =
    "同一份数据形状、同一缓存额度下与 RocksDB、fjall 对拍；下表为同一台机器、同一轮次的实测吞吐。";
  header.prepend(badge);
  header.append(sub);

  const perf_sec = section("性能矩阵"),
    meta = newEl("p"),
    table_wrap = newEl("div"),
    table = newEl("table"),
    thead = newEl("thead"),
    tbody = newEl("tbody"),
    tr_head = newEl("tr"),
    dash = (value) => (value == null || value === "" ? "—" : value);

  perf_sec.id = "perf";
  meta.className = "HomeMeta";
  meta.textContent = PERF_META;
  perf_sec.append(meta);

  table_wrap.className = "HomeTableWrap";
  table.className = "HomeTable";
  tr_head.append(
    cell("th", "阶段"),
    cell("th", "Rust fkv"),
    cell("th", "RocksDB"),
    cell("th", "fjall"),
    cell("th", "fkv / RocksDB"),
    cell("th", "fkv / fjall")
  );
  thead.append(tr_head);
  // 倍率单元格各自着色：ratio > 1（fkv 更快）蓝色，< 0.5（慢一倍以上）红色，
  // 0.5 ~ 1 之间保持中性——只有真的更快的格子才是加粗的。
  const ratio_el = (text) => {
    const el = cell("td", dash(text)),
      value = parseFloat(text);
    if (text != null && value >= 1) el.className = "HomeSpeed";
    else if (text != null && value < 0.5) el.className = "HomeSlow";
    return el;
  };
  for (const [phase, fkv, rocksdb, fjall, ratio_rocksdb, ratio_fjall]
    of PERF_MATRIX_ROWS) {
    const tr = newEl("tr");
    tr.append(
      cell("td", phase),
      cell("td", dash(fkv)),
      cell("td", dash(rocksdb)),
      cell("td", dash(fjall)),
      ratio_el(ratio_rocksdb),
      ratio_el(ratio_fjall)
    );
    tbody.append(tr);
  }
  table.append(thead, tbody);
  table_wrap.append(table);

  const note = newEl("p");
  note.className = "HomeNote";
  const note_link = newEl("a");
  note_link.href = "/doc/performance";
  note_link.textContent = "查看口径";
  note.append(PERF_NOTE + " ", note_link);

  // 表下配置分四块（机器 / 测试负载 / 持久化 / RocksDB 列），各自成卡。
  const config_area = newEl("div");
  config_area.className = "HomeConfig";
  for (const [title, item_li] of Object.entries(PERF_CONFIG)) {
    const card = newEl("div");
    card.className = "HomeConfigCard";
    const h3 = newEl("h3");
    h3.textContent = title;
    const dl = newEl("dl");
    for (const [name, value] of item_li) {
      const dt = newEl("dt");
      dt.textContent = name;
      const dd = newEl("dd");
      dd.textContent = value;
      dl.append(dt, dd);
    }
    card.append(h3, dl);
    config_area.append(card);
  }

  perf_sec.append(table_wrap, config_area, note);

  const verify_sec = section("验证状态"),
    verify_bar = newEl("div");
  verify_bar.className = "HomeVerify";
  verify_bar.append(
    ...VERIFY_ROWS.map(([name, value]) => {
      const item = newEl("div"),
        label = newEl("span");
      item.className = "HomeVerifyItem";
      label.textContent = name;
      item.append(cell("b", value), label);
      return item;
    })
  );
  verify_sec.append(verify_bar);

  const doc_sec = section("文档"),
    docs = newEl("nav");
  docs.className = "HomeLinks";
  docs.append(
    ...DOC_LINKS.map(([key, title]) => {
      const a = newEl("a");
      a.href = "/doc/" + key;
      a.textContent = title;
      return a;
    })
  );
  doc_sec.append(docs);

  const footer = newEl("footer"),
    github = newEl("a"),
    license = newEl("a");

  footer.className = "HomeFoot";
  github.href = GITHUB;
  github.textContent = "GitHub ↗";
  license.href = GITHUB + "/blob/main/LICENSE";
  license.textContent = "Apache-2.0";
  footer.append(github, license);

  el.append(header, perf_sec, verify_sec, doc_sec, footer);
  res_li = [Promise.resolve(el), () => {}];
  return res_li;
};

export default (url) => {
  if (!url) {
    return res_li || page();
  }
};

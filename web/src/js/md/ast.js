import { htmD, isPunct } from "./htmD.js";
export const I_TYPE = 0,
  I_CONTENT = 2,
  I_ATTR1 = 3,
  I_ATTR2 = 4,
  I_ATTR3 = 5,
  T_H = 2,
  T_P = 3,
  T_CODE_BLOCK = 4,
  T_BLOCKQUOTE = 5,
  T_LIST = 6,
  T_LIST_ITEM = 7,
  T_THEMATIC_BREAK = 8,
  T_TABLE = 9,
  T_HTML = 10,
  T_MATH_BLOCK = 11,
  T_TEXT = 12,
  T_STRONG = 13,
  T_EM = 14,
  T_CODE = 15,
  T_LINK = 16,
  T_IMG = 17,
  T_DEL = 18,
  T_BR = 19,
  T_MATH_INLINE = 20;

const BLOCK_TAG_REG =
    /^<\/?(address|article|aside|base|basefont|blockquote|body|caption|center|col|colgroup|dd|details|dialog|dir|div|dl|dt|fieldset|figcaption|figure|footer|form|frame|frameset|h[1-6]|head|header|hr|html|iframe|legend|li|link|main|menu|menuitem|nav|noframes|ol|optgroup|option|p|param|section|source|summary|table|tbody|td|tfoot|th|thead|title|tr|track|ul)(?:\s|>|\/|$)/i,
  normLabel = (s) => htmD(s, 0).toLowerCase().replace(/\s+/g, " ").trim(),
  // 状态常量
  ST_NEW = 0,
  ST_UPDATE = 1,
  I_STATUS = 1,
  ST_DONE = 2,
  isSpace = (c) => c === 32 || c === 9 || c === 10 || c === 13 || c === 160,
  skipHWs = (str, p) => {
    while (str[p] === " " || str[p] === "\t") ++p;
    return p;
  },
  parseNextLinkRefDef = (text, start_pos) => {
    const len = text.length;
    let p = start_pos;
    const m = text.slice(p).match(/^ {0,3}\[((?:\\.|[^[\]]){1,999})\]:/);
    if (!m || m[1].trim() === "") return;
    const label = m[1];
    p += m[0].length;
    p += text.slice(p).match(/^[ \t\n\r]*/)[0].length;
    if (p >= len) return;

    let url = "";
    if (text[p] === "<") {
      ++p;
      const url_start = p;
      while (p < len && text[p] !== ">" && text[p] !== "\n" && text[p] !== "\r") {
        p += text[p] === "\\" && p + 1 < len ? 2 : 1;
      }
      if (p >= len || text[p] !== ">") return;
      url = text.slice(url_start, p);
      ++p;
    } else {
      const url_start = p;
      let paren = 0;
      while (p < len) {
        const c = text[p];
        if (c <= " ") break;
        if (c === "\\") {
          p += 2;
          continue;
        }
        if (c === "(") ++paren;
        else if (c === ")") {
          if (!paren--) break;
        }
        ++p;
      }
      if (paren > 0) return;
      url = text.slice(url_start, p);
    }

    const tryTitle = (tp) => {
        const tc = text[tp];
        if (tc === '"' || tc === "'" || tc === "(") {
          const quote = tc === "(" ? ")" : tc;
          let sp = tp + 1;
          const ts = sp;
          while (sp < len && text[sp] !== quote) {
            sp += text[sp] === "\\" && sp + 1 < len ? 2 : 1;
          }
          if (sp < len && text[sp] === quote) {
            const cp = skipHWs(text, sp + 1),
              cc = text[cp];
            if (!cc || cc === "\n" || cc === "\r") {
              return [text.slice(ts, sp), cp + (cc === "\r" && text[cp + 1] === "\n" ? 2 : 1)];
            }
          }
        }
        return;
      },
      title_p = skipHWs(text, p),
      tc = text[title_p];
    if (!tc || tc === "\n" || tc === "\r") {
      const next_line_start = title_p + (tc === "\r" && text[title_p + 1] === "\n" ? 2 : 1),
        tt = next_line_start > title_p ? tryTitle(skipHWs(text, next_line_start)) : null;
      return tt ? [label, url, tt[0], tt[1]] : [label, url, "", next_line_start];
    }
    if (title_p > p) {
      const tt = tryTitle(title_p);
      if (tt) return [label, url, tt[0], tt[1]];
    }
    return;
  },
  extractLrdFromText = (text, link_def_li) => {
    let p = 0,
      res;
    while ((res = parseNextLinkRefDef(text, p))) {
      const norm = normLabel(res[0]);
      if (!link_def_li.some((d) => d[0] === norm)) {
        link_def_li.push([norm, res[1], res[2]]);
      }
      p = res[3];
    }
    return p;
  },
  extractLinkDefs = (node_li, link_def_li) => {
    let i = 0;
    while (i < node_li.length) {
      const node = node_li[i];
      if (node._extracted) {
        ++i;
        continue;
      }
      if (node[I_TYPE] === T_P) {
        const text = node[I_CONTENT];
        if (typeof text === "string") {
          const p = extractLrdFromText(text, link_def_li);
          if (p > 0) {
            const remaining = text.slice(p);
            if (remaining.trim() === "") {
              node_li.splice(i, 1);
              continue;
            } else {
              node[I_CONTENT] = remaining;
            }
          }
        }
      } else if (Array.isArray(node[I_CONTENT])) {
        extractLinkDefs(node[I_CONTENT], link_def_li);
      }
      if (node[I_STATUS] === ST_DONE) {
        node._extracted = 1;
      }
      ++i;
    }
  },
  parseHtmlBlockStart = (line) => {
    if (line.charCodeAt(0) !== 60) return 0;
    if (/^<(script|pre|style|textarea)(?:\s|>|\/|$)/i.test(line)) return 1;
    if (line.startsWith("<!--")) return 2;
    if (line.startsWith("<?")) return 3;
    if (/^<![A-Z]/.test(line)) return 4;
    if (line.startsWith("<![CDATA[")) return 5;
    if (BLOCK_TAG_REG.test(line)) return 6;
    return /^(?:<\/[a-zA-Z][a-zA-Z0-9-]*\s*>|<[a-zA-Z][a-zA-Z0-9-]*(?:\s+[a-zA-Z:_][a-zA-Z0-9_.:-]*(?:\s*=\s*(?:"[^"]*"|'[^']*'|[^\s"'=<>`]+))?)*\s*\/?>)$/.test(
      line
    )
      ? 7
      : 0;
  },
  checkHtmlBlockEnd = (line, type) =>
    type === 1
      ? /<\/(?:script|pre|style|textarea)>/i.test(line)
      : type >= 2 && type <= 5
        ? line.includes(["", "", "-->", "?>", ">", "]]>"][type])
        : 0,
  parseSetextUnderline = (line) => {
    const m = line.match(/^[ \t]{0,3}(=+|-+)[ \t]*$/);
    return m ? (m[1][0] === "=" ? 1 : 2) : 0;
  },
  parseInlineHtml = (text, start) => {
    const m = text
      .slice(start)
      .match(
        /^<!--(?:>|->|[\s\S]*?-->)|^<!\[CDATA\[[\s\S]*?\]\]>|^<![A-Z]+[^>]*>|^<\?[\s\S]*?\?>|^<\/[a-zA-Z][a-zA-Z0-9-]*\s*>|^<[a-zA-Z][a-zA-Z0-9-]*(?:\s+[a-zA-Z:_][a-zA-Z0-9_.:-]*(?:\s*=\s*(?:"[^"]*"|'[^']*'|[^\s"'=<>`]+))?)*\s*\/?>/
      );
    return m ? m[0] : null;
  },
  parseAutolink = (text, start) => {
    const m = text.slice(start).match(/^<([^<>]+)>/);
    if (m) {
      const content = m[1];
      if (/^[a-zA-Z][a-zA-Z\d+.-]{1,31}:[^<>\s]*$/.test(content)) {
        return [1, content];
      }
      if (
        /^[a-zA-Z\d.!#$%&'*+/=?^_`{|}~-]+@[a-zA-Z\d](?:[a-zA-Z\d-]*[a-zA-Z\d])?(?:\.[a-zA-Z\d](?:[a-zA-Z\d-]*[a-zA-Z\d])?)*$/.test(
          content
        )
      ) {
        return [2, content];
      }
    }
    return;
  },
  // Inline 节点深度 Emphasis 处理
  processEmphasis = (nodes) => {
    const stack = [];
    let p = 0;
    while (p < nodes.length) {
      const item = nodes[p];
      if (item[I_TYPE] === 100 && item[5]) {
        // can_close
        const char = item[I_CONTENT],
          count = item[3];
        let open_idx = -1;
        for (let idx = stack.length - 1; idx >= 0; --idx) {
          const o = stack[idx];
          if (o[0] === 100 && o[2] === char && o[4]) {
            // can_open — apply rule of three
            if ((item[4] || o[5]) && (o[6] + item[6]) % 3 === 0 && o[6] % 3) {
              continue;
            }
            open_idx = idx;
            break;
          }
        }
        if (open_idx !== -1) {
          const o = stack[open_idx],
            use_count = o[3] >= 2 && count >= 2 ? 2 : 1,
            inner = [];
          for (let idx = open_idx + 1; idx < stack.length; ++idx) {
            const node = stack[idx];
            if (node[0] === 100) {
              inner.push([T_TEXT, ST_DONE, String.fromCharCode(node[2]).repeat(node[3])]);
            } else {
              inner.push(node);
            }
          }
          const type = char === 126 ? T_DEL : use_count === 2 ? T_STRONG : T_EM;
          // Dissolve nested same-type strong emphasis (e.g. <strong><strong>x</strong></strong> → <strong>x</strong>)
          if (type === T_STRONG) {
            for (let di = inner.length - 1; di >= 0; --di) {
              const ch = inner[di];
              if (ch[I_TYPE] === T_STRONG && Array.isArray(ch[I_CONTENT])) {
                inner.splice(di, 1, ...ch[I_CONTENT]);
              }
            }
          }
          const new_node = [type, ST_DONE, inner];
          stack.length = open_idx;
          if (o[3] > use_count) {
            o[3] -= use_count;
            stack.push(o);
          }
          stack.push(new_node);
          if (item[3] > use_count) {
            item[3] -= use_count;
            continue;
          }
        } else {
          stack.push(item);
        }
      } else {
        stack.push(item);
      }
      ++p;
    }

    const final_nodes = [];
    for (const node of stack) {
      const is_100 = node[0] === 100;
      if (is_100 || node[0] === T_TEXT) {
        const txt = is_100
            ? String.fromCharCode(node[2]).repeat(node[3])
            : node[3]
              ? node[I_CONTENT]
              : htmD(node[I_CONTENT]),
          last = final_nodes[final_nodes.length - 1];
        if (last && last[I_TYPE] === T_TEXT) {
          last[I_CONTENT] += txt;
        } else {
          final_nodes.push(is_100 ? [T_TEXT, ST_DONE, txt] : [T_TEXT, ST_DONE, txt, node[3]]);
        }
      } else {
        if (Array.isArray(node[I_CONTENT])) {
          node[I_CONTENT] = processEmphasis(node[I_CONTENT]);
        }
        final_nodes.push(node);
      }
    }
    return final_nodes;
  },
  plainText = (node) =>
    node[I_TYPE] >= 10 && node[I_TYPE] < 100
      ? Array.isArray(node[I_CONTENT])
        ? node[I_CONTENT].map(plainText).join("")
        : node[I_CONTENT]
      : "",
  // Inline 扫描状态机主入口
  parseInline = (text, link_defs = []) => {
    if (!text) return [];
    const findLinkDef = (label) => {
        const norm = normLabel(label);
        return link_defs.find((d) => d[0] === norm) || null;
      },
      nodes = [],
      len = text.length,
      finishLink = (marker_idx, url, title) => {
        const marker = nodes[marker_idx],
          inner_nodes = nodes.slice(marker_idx + 1);
        nodes.length = marker_idx;
        if (marker[I_TYPE] === T_IMG) {
          nodes.push([T_IMG, ST_DONE, inner_nodes.map(plainText).join(""), url, title]);
        } else {
          nodes.push([T_LINK, ST_DONE, inner_nodes, url, title]);
          for (let idx = 0; idx < marker_idx; ++idx) {
            if (nodes[idx][I_TYPE] === T_LINK && nodes[idx][I_STATUS] === ST_NEW) {
              nodes[idx][I_STATUS] = ST_DONE;
            }
          }
        }
      };
    let i = 0,
      last_text_start = 0;

    const pushText = (end) => {
      if (last_text_start < end) {
        nodes.push([T_TEXT, ST_DONE, text.slice(last_text_start, end)]);
      }
    };

    while (i < len) {
      const c = text.charCodeAt(i),
        pushBr = (len) => {
          pushText(i);
          nodes.push([T_BR, ST_DONE, ""]);
          last_text_start = i += len;
        };
      if (c === 10) {
        pushBr(1);
        continue;
      }
      if (c === 32) {
        const m = text.slice(i).match(/^ {2,}\n/);
        if (m) {
          pushBr(m[0].length);
          continue;
        }
      }
      if (c === 92) {
        if (text.charCodeAt(i + 1) === 10) {
          pushBr(2);
          continue;
        }
        i += 2;
        continue;
      }
      if (c === 42 || c === 95 || c === 126) {
        pushText(i);
        let j = i;
        while (j < len && text.charCodeAt(j) === c) ++j;
        const count = j - i;
        if (c === 126 && count !== 2) {
          i = j;
          last_text_start = i;
          continue;
        }
        const prev_char = i > 0 ? text.charCodeAt(i - 1) : 32,
          next_char = j < len ? text.charCodeAt(j) : 32,
          is_space_prev = isSpace(prev_char),
          is_space_next = isSpace(next_char),
          is_punct_prev = isPunct(prev_char),
          is_punct_next = isPunct(next_char),
          left_flanking = !is_space_next && (!is_punct_next || is_space_prev || is_punct_prev),
          right_flanking = !is_space_prev && (!is_punct_prev || is_space_next || is_punct_next);

        let can_open = left_flanking,
          can_close = right_flanking;

        if (c === 95) {
          can_open = left_flanking && (!right_flanking || is_punct_prev);
          can_close = right_flanking && (!left_flanking || is_punct_next);
        }
        nodes.push([100, ST_NEW, c, count, can_open, can_close, count]);
        i = j;
        last_text_start = i;
        continue;
      }
      if (c === 60) {
        const html_tag = parseInlineHtml(text, i);
        if (html_tag) {
          pushText(i);
          nodes.push([T_HTML, ST_DONE, html_tag]);
          i += html_tag.length;
          last_text_start = i;
          continue;
        }

        const autolink = parseAutolink(text, i);
        if (autolink) {
          pushText(i);
          const type = autolink[0],
            content = autolink[1],
            url = type === 2 ? "mailto:" + content : content;
          nodes.push([T_LINK, ST_DONE, [[T_TEXT, ST_DONE, content, 1]], url, null]);
          i += content.length + 2;
          last_text_start = i;
          continue;
        }
      }
      // Inline Code / Math Inline — shared delimiter parsing
      if (c === 96 || c === 36) {
        pushText(i);
        let j = i;
        while (j < len && text.charCodeAt(j) === c) ++j;
        const count = j - i;
        let k = j,
          found = -1;
        while (k < len) {
          if (c === 36 && text.charCodeAt(k) === 92) {
            k += 2;
            continue;
          }
          if (text.charCodeAt(k) === c) {
            let m = k;
            while (m < len && text.charCodeAt(m) === c) ++m;
            if (m - k === count) {
              found = k;
              break;
            }
            k = m;
          } else {
            ++k;
          }
        }
        if (found !== -1) {
          let processed = text.slice(j, found);
          if (c === 96) {
            let r = processed.replace(/\r\n?|\n/g, " ");
            if (r[0] === " " && r.at(-1) === " " && r.trim()) {
              r = r.slice(1, -1);
            }
            processed = r;
          }
          nodes.push([c === 96 ? T_CODE : T_MATH_INLINE, ST_DONE, processed]);
          i = found + count;
          last_text_start = i;
          continue;
        } else {
          last_text_start = i;
          i = j;
          continue;
        }
      }
      // Link / Image Open: [
      if (c === 91) {
        let is_img = 0;
        if (text.charCodeAt(i - 1) === 33) {
          let bs = 0;
          while (text.charCodeAt(i - 2 - bs) === 92) ++bs;
          is_img = bs % 2 === 0;
        }
        pushText(is_img ? i - 1 : i);
        nodes.push([is_img ? T_IMG : T_LINK, ST_NEW, i, 1]);
        last_text_start = i + 1;
        ++i;
        continue;
      }
      // Link / Image Close: ]
      if (c === 93) {
        let marker_idx = -1;
        for (let idx = nodes.length - 1; idx >= 0; --idx) {
          if (
            (nodes[idx][I_TYPE] === T_LINK || nodes[idx][I_TYPE] === T_IMG) &&
            nodes[idx][3] === 1
          ) {
            marker_idx = idx;
            break;
          }
        }
        if (marker_idx !== -1) {
          if (nodes[marker_idx][I_STATUS] === ST_NEW) {
            let next = i + 1;

            let is_inline = 0;
            if (next < len && text.charCodeAt(next) === 40) {
              const m = text
                .slice(next)
                .match(
                  /^\([ \t\n\r]*(?:<((?:\\.|[^>\\\n])*)>|((?:\\.|[^<() \t\n\r]|\((?:\\.|[^() \t\n\r]|\((?:\\.|[^() \t\n\r])*\))*\))*)?)(?:[ \t\n\r]+("([^"\\]*(?:\\.[^"\\]*)*)"|'([^'\\]*(?:\\.[^'\\]*)*)'|\(([^)\\]*(?:\\.[^)\\]*)*)\)))?[ \t\n\r]*\)/
                );
              if (m) {
                const url = m[1] != null ? m[1] : m[2] || "",
                  title = m[4] != null ? m[4] : m[5] != null ? m[5] : m[6] || "";
                pushText(i);
                finishLink(marker_idx, htmD(url), htmD(title));
                i = next + m[0].length;
                last_text_start = i;
                is_inline = 1;
              }
            }

            if (is_inline) {
              continue;
            }

            // Reference link!
            let ref_label = "";
            let ref_len = 0;
            const ref_m =
              next < len && text.charCodeAt(next) === 91
                ? text.slice(next).match(/^\[((?:\\.|[^[\]])*)\]/)
                : null;
            if (ref_m) {
              ref_label =
                ref_m[1].trim() === "" ? text.slice(nodes[marker_idx][2] + 1, i) : ref_m[1];
              ref_len = ref_m[0].length;
            } else {
              ref_label = text.slice(nodes[marker_idx][2] + 1, i);
            }

            // Look up ref_label
            const label_ok =
                ref_label.trim() !== "" && !/\]|\[/.test(ref_label.replace(/\\./g, "")),
              def = label_ok ? findLinkDef(ref_label) : null;
            if (def) {
              pushText(i);
              finishLink(marker_idx, htmD(def[1]), htmD(def[2]));
              i = ref_m ? next + ref_len : i + 1;
              last_text_start = i;
              continue;
            }
          }

          const marker = nodes[marker_idx],
            orig_type = marker[I_TYPE];
          marker[I_TYPE] = T_TEXT;
          marker[I_CONTENT] = orig_type === T_IMG ? "![" : "[";
          marker[I_STATUS] = ST_DONE;
        }
      }
      ++i;
    }
    pushText(len);
    const cleaned_nodes = [];
    for (const node of nodes) {
      if ((node[I_TYPE] === T_LINK || node[I_TYPE] === T_IMG) && node[3] === 1) {
        cleaned_nodes.push([T_TEXT, ST_DONE, node[I_TYPE] === T_IMG ? "![" : "["]);
      } else {
        cleaned_nodes.push(node);
      }
    }
    return processEmphasis(cleaned_nodes);
  },
  // 辅助检测 block start
  isThematicBreak = (line) => /^[ \t]*([*_-])(?:[ \t]*\1){2,}[ \t]*$/.test(line),
  parseHeading = (line) => {
    const m = line.match(/^(#{1,6})(?:[ \t]|$)/);
    if (!m) return;
    let content = line.slice(m[0].length).trim();
    content = content.replace(/(^|[ \t])#+\s*$/, "").trim();
    return [m[1].length, content];
  },
  parseCodeFence = (line) => {
    const m = line.match(/^(`{3,}|~{3,})(.*)/);
    if (!m) return;
    const char = m[1][0];
    let lang = m[2].trim();
    if (char === "`" && lang.includes("`")) return;
    return [char, m[1].length, lang.split(/\s/)[0]];
  },
  parseListMarker = (line) => {
    const m = line.match(/^(?:([-*+])|(\d{1,9})([.)]))([ \t]|$)/);
    if (!m) return;
    return m[1] ? [1, m[1], 0, 1] : [2, m[3], +m[2], m[2].length + 1];
  },
  // 检查是否是 Table border 行
  parseTableBorder = (line) => {
    if (!line.includes("|")) return;
    const parts = line.split("|"),
      start = parts[0].trim() ? 0 : 1,
      end = parts.at(-1).trim() ? parts.length : parts.length - 1;
    if (end <= start) return;

    const aligns = [];
    for (let i = start; i < end; ++i) {
      const m = parts[i].trim().match(/^(:?)-+(:?)$/);
      if (!m) return;
      aligns.push(m[1] ? (m[2] ? "center" : "left") : m[2] ? "right" : "");
    }
    return aligns;
  },
  // 拆分 Table 行单元格
  splitTableCells = (line) =>
    line
      .trim()
      .replace(/^\||\|$/g, "")
      .split(/(?<!\\)\|/)
      .map((c) => c.replace(/\\\|/g, "|").trim());

// 核心流式解析器
export default (initial_text) => {
  let raw_text = "",
    blocks = [],
    stack = [],
    link_defs = [],
    has_blank_line = 0;
  const markListLoose = () => {
      for (let idx = stack.length - 1; idx >= 0; --idx) {
        if (stack[idx][I_TYPE] === T_LIST) {
          stack[idx][5] = 0;
          break;
        }
      }
    },
    mkList = (l_marker) => [
      T_LIST,
      ST_NEW,
      [],
      l_marker[0],
      l_marker[0] === 1 ? l_marker[1] : l_marker[2],
      1,
      l_marker[1]
    ],
    addHtmlBlock = (sub_line, html_type) => {
      const html_node = [T_HTML, ST_NEW, sub_line + "\n", html_type];
      addBlock(html_node);
      if (checkHtmlBlockEnd(sub_line, html_type)) {
        html_node[I_STATUS] = ST_DONE;
      }
    },
    canListInterrupt = (l_marker, sub_trimmed, is_interrupting_p) => {
      let lastList = null;
      for (let idx = stack.length - 1; idx >= 0; --idx) {
        if (stack[idx][I_TYPE] === T_LIST) {
          lastList = stack[idx];
          break;
        }
      }
      if (l_marker[0] === 2 && l_marker[2] !== 1) {
        if (is_interrupting_p) return;
        if (!lastList || lastList[I_ATTR1] !== l_marker[0]) return;
      }
      const content_str = sub_trimmed.slice(l_marker[3]).trim();
      if (content_str === "") {
        if (!lastList || lastList[I_ATTR1] !== l_marker[0] || lastList[6] !== l_marker[1]) {
          return;
        }
      }
      return 1;
    },
    extractLeadingLrd = (p_node) => {
      let p_text = p_node[I_CONTENT],
        start = extractLrdFromText(p_text, link_defs);
      if (start > 0) {
        p_node[I_CONTENT] = p_text.slice(start);
        if (p_node[I_CONTENT].trim() === "") {
          let parent_li = blocks;
          for (let idx = stack.length - 1; idx >= 0; --idx) {
            if (Array.isArray(stack[idx][I_CONTENT])) {
              parent_li = stack[idx][I_CONTENT];
              break;
            }
          }
          const p_idx = parent_li.indexOf(p_node);
          if (p_idx !== -1) {
            parent_li.splice(p_idx, 1);
          }
        }
      }
    },
    getActiveBlock = () => {
      let curr = null;
      if (stack.length) {
        curr = stack[stack.length - 1];
      } else if (blocks.length) {
        curr = blocks[blocks.length - 1];
      }
      while (curr) {
        const type = curr[I_TYPE];
        if (type < T_BLOCKQUOTE || type > T_LIST_ITEM) {
          return curr;
        }
        const children = curr[I_CONTENT];
        if (Array.isArray(children) && children.length) {
          curr = children[children.length - 1];
        } else {
          return null;
        }
      }
      return;
    },
    pushStack = (node) => {
      if (has_blank_line) {
        markListLoose();
        has_blank_line = 0;
      }
      stack.push(node);
    },
    closeActiveBlocks = (depth) => {
      while (stack.length > depth) {
        const top = stack.pop();
        if (top[I_TYPE] === T_LIST) {
          if (has_blank_line) {
            markListLoose();
          }
          has_blank_line = 0;
        }
        top[I_STATUS] = ST_DONE;
        if (Array.isArray(top[I_CONTENT])) {
          for (const child of top[I_CONTENT]) {
            child[I_STATUS] = ST_DONE;
          }
        }
      }
    },
    feedLine = (orig_line, is_continuation = 0) => {
      let line = orig_line;
      if (orig_line.includes("\t")) {
        line = "";
        let col = 0,
          in_prefix = 1;
        for (const c of orig_line) {
          if (in_prefix && c === "\t") {
            const spaces = 4 - (col % 4);
            line += " ".repeat(spaces);
            col += spaces;
          } else {
            line += c;
            ++col;
            if (in_prefix && !/[ \t>\-+*\d.)]/.test(c)) in_prefix = 0;
          }
        }
      }

      const line_indent = line.match(/^ */)[0].length;
      let line_pos = 0,
        rem_indent = line_indent,
        depth = 0;

      if (is_continuation) {
        depth = stack.length;
      } else {
        while (depth < stack.length) {
          const container = stack[depth],
            type = container[I_TYPE];

          if (type === T_BLOCKQUOTE) {
            const m = line.slice(line_pos).match(/^( *)> ?/);
            if (m) {
              line_pos += m[0].length;
              rem_indent = line.slice(line_pos).match(/^ */)[0].length;
              ++depth;
            } else {
              break;
            }
          } else if (type === T_LIST) {
            ++depth;
          } else if (type === T_LIST_ITEM) {
            const start_indent = container[6] || 0;
            if (rem_indent >= start_indent) {
              rem_indent -= start_indent;
              line_pos += start_indent;
              ++depth;
            } else {
              break;
            }
          } else {
            ++depth;
          }
        }
      }

      const baseIndent = (limit) => {
          for (let k = limit - 1; k >= 0; --k) {
            if (stack[k][I_TYPE] === T_LIST_ITEM) {
              return is_continuation ? 0 : stack[k][6] || 0;
            }
          }
          return 0;
        },
        sub_line = line.slice(line_pos),
        sub_trimmed = sub_line.trim(),
        sub_is_blank = sub_trimmed === "";

      let is_lazy = 0;
      if (!is_continuation && depth < stack.length) {
        const active = getActiveBlock();
        if (active && active[I_TYPE] === T_P && active[I_STATUS] !== ST_DONE && !sub_is_blank) {
          const base_indent = baseIndent(depth),
            lm = rem_indent < base_indent + 4 && parseListMarker(sub_trimmed),
            ht = line_indent < 4 && parseHtmlBlockStart(sub_trimmed);
          if (
            !(
              (rem_indent < base_indent + 4 &&
                (isThematicBreak(sub_trimmed) ||
                  parseHeading(sub_trimmed) ||
                  parseCodeFence(sub_trimmed) ||
                  sub_trimmed.startsWith(">") ||
                  (lm && canListInterrupt(lm, sub_trimmed, 0)))) ||
              (ht >= 1 && ht <= 6)
            )
          ) {
            is_lazy = 1;
          }
        }
      }

      let has_unmatched_blockquote = 0;
      for (let k = depth; k < stack.length; ++k) {
        if (stack[k][I_TYPE] === T_BLOCKQUOTE) {
          has_unmatched_blockquote = 1;
          break;
        }
      }
      if (!is_lazy && depth < stack.length && (!sub_is_blank || has_unmatched_blockquote)) {
        closeActiveBlocks(depth);
      }

      const base_indent = baseIndent(is_lazy ? depth : stack.length);
      // rem_indent is already progressively reduced inside the container matching loop

      let active = getActiveBlock();

      if (active && active[I_STATUS] !== ST_DONE) {
        const a_type = active[I_TYPE];

        if (a_type === T_P) {
          const level = parseSetextUnderline(sub_trimmed);
          if (level > 0 && line_indent < 4 && depth === stack.length) {
            extractLeadingLrd(active);
            if (active[I_CONTENT].trim() !== "") {
              active[I_TYPE] = T_H;
              active[I_STATUS] = ST_DONE;
              active[I_ATTR1] = level;
              return;
            } else {
              active = null;
            }
          }
        }

        if (a_type === T_HTML) {
          const type = active[I_ATTR1];
          if (sub_is_blank && (type === 6 || type === 7)) {
            active[I_STATUS] = ST_DONE;
          } else {
            active[I_CONTENT] += sub_line + "\n";
            if (checkHtmlBlockEnd(sub_line, type)) {
              active[I_STATUS] = ST_DONE;
            }
            return;
          }
        }

        if (a_type === T_CODE_BLOCK && active[I_ATTR1] === null) {
          if (rem_indent >= 4 || sub_is_blank) {
            active[I_CONTENT] += "\n" + (rem_indent >= 4 ? line.slice(line_pos + 4) : "");
            active[I_STATUS] = ST_UPDATE;
            return;
          }
          active[I_STATUS] = ST_DONE;
          active = null;
        }

        if (active && active[I_STATUS] !== ST_DONE && active[I_TYPE] === T_CODE_BLOCK) {
          const fence_char = active[I_ATTR1],
            fence_count = active[I_ATTR2],
            fence = rem_indent < base_indent + 4 ? parseCodeFence(sub_trimmed) : null;
          if (fence && fence[0] === fence_char && fence[1] >= fence_count && fence[2] === "") {
            active[I_STATUS] = ST_DONE;
          } else {
            let content = sub_line;
            const fence_indent = active[6];
            if (fence_indent > 0) {
              let s_idx = 0;
              while (
                s_idx < sub_line.length &&
                s_idx < fence_indent &&
                sub_line.charCodeAt(s_idx) === 32
              ) {
                ++s_idx;
              }
              content = sub_line.slice(s_idx);
            }
            if (active[I_CONTENT] === null) {
              active[I_CONTENT] = content;
            } else {
              active[I_CONTENT] += "\n" + content;
            }
            active[I_STATUS] = ST_UPDATE;
          }
          return;
        }

        if (a_type === T_MATH_BLOCK) {
          if (sub_trimmed === "$$") {
            active[I_STATUS] = ST_DONE;
          } else {
            if (active[I_CONTENT] === null) {
              active[I_CONTENT] = sub_line;
            } else {
              active[I_CONTENT] += "\n" + sub_line;
            }
            active[I_STATUS] = ST_UPDATE;
          }
          return;
        }
      }

      if (!active || active[I_STATUS] === ST_DONE) {
        if (rem_indent >= 4 && !sub_is_blank) {
          const actual_pos = line_pos + 4,
            content = line.slice(actual_pos),
            ind_code_node = [T_CODE_BLOCK, ST_NEW, content, null, 0, null];
          addBlock(ind_code_node);
          return;
        }
      }

      if (sub_is_blank) {
        // Only mark as blank line for list looseness if not inside a blockquote
        // within a list item. A blank within a blockquote shouldn't make the list loose.
        let inside_bq_in_li = 0;
        for (let k = stack.length - 1; k >= 0; --k) {
          if (stack[k][I_TYPE] === T_BLOCKQUOTE) {
            inside_bq_in_li = 1;
            break;
          }
          if (stack[k][I_TYPE] === T_LIST_ITEM) break;
        }
        if (!inside_bq_in_li) has_blank_line = 1;
        if (stack.length && stack[stack.length - 1][I_TYPE] === T_LIST_ITEM) {
          const last_item = stack[stack.length - 1];
          if (!Array.isArray(last_item[I_CONTENT]) || !last_item[I_CONTENT].length) {
            closeActiveBlocks(stack.length - 1);
          }
        }
        if (active && active[I_TYPE] !== T_LIST && active[I_TYPE] !== T_BLOCKQUOTE) {
          if (active[I_TYPE] === T_HTML && active[I_ATTR1] >= 1 && active[I_ATTR1] <= 5) {
            active[I_CONTENT] += "\n";
            return;
          }
          active[I_STATUS] = ST_DONE;
        }
        return;
      }

      if (rem_indent < base_indent + 4) {
        if (sub_trimmed.startsWith("$$")) {
          if (sub_trimmed.endsWith("$$") && sub_trimmed.length > 2) {
            addBlock([T_MATH_BLOCK, ST_DONE, sub_trimmed.slice(2, -2).trim()]);
            return;
          }
          if (sub_trimmed === "$$") {
            addBlock([T_MATH_BLOCK, ST_NEW, null]);
            return;
          }
        }

        const fence = parseCodeFence(sub_trimmed);
        if (fence) {
          const fence_indent = rem_indent,
            c_node = [T_CODE_BLOCK, ST_NEW, null, fence[0], fence[1], htmD(fence[2]), fence_indent];
          addBlock(c_node);
          return;
        }

        const h_info = parseHeading(sub_trimmed);
        if (h_info) {
          const h_node = [T_H, ST_DONE, h_info[1], h_info[0]];
          addBlock(h_node);
          return;
        }

        if (isThematicBreak(sub_trimmed)) {
          const hr_node = [T_THEMATIC_BREAK, ST_DONE, ""];
          addBlock(hr_node);
          return;
        }

        if (sub_trimmed.startsWith(">")) {
          const b_node = [T_BLOCKQUOTE, ST_NEW, []];
          addBlock(b_node);
          pushStack(b_node);
          // Strip the ">" marker and optional following space from sub_line
          let bq_pos = 0;
          while (bq_pos < sub_line.length && sub_line.charCodeAt(bq_pos) !== 62) ++bq_pos;
          let bq_skip = 1;
          if (bq_pos + 1 < sub_line.length && sub_line.charCodeAt(bq_pos + 1) === 32) bq_skip = 2;
          feedLine(sub_line.slice(bq_pos + bq_skip), 1);
          return;
        }

        const l_marker = parseListMarker(sub_trimmed);
        if (l_marker) {
          const is_interrupting_p =
              active && active[I_TYPE] === T_P && active[I_STATUS] !== ST_DONE,
            can_interrupt = !is_interrupting_p || canListInterrupt(l_marker, sub_trimmed, 1);
          if (can_interrupt) {
            let list = null;
            if (stack.length) {
              const lastList = stack[stack.length - 1];
              if (
                lastList[I_TYPE] === T_LIST &&
                lastList[I_STATUS] !== ST_DONE &&
                lastList[I_ATTR1] === l_marker[0] &&
                lastList[6] === l_marker[1]
              ) {
                list = lastList;
              }
            }
            if (!list) {
              const parentChildren = stack.length ? stack[stack.length - 1][I_CONTENT] : blocks;
              if (parentChildren.length) {
                const lastList = parentChildren[parentChildren.length - 1];
                if (
                  lastList[I_TYPE] === T_LIST &&
                  lastList[I_STATUS] !== ST_DONE &&
                  lastList[I_ATTR1] === l_marker[0] &&
                  lastList[6] === l_marker[1]
                ) {
                  list = lastList;
                }
              }
            }

            if (list) {
              const list_idx = stack.indexOf(list);
              if (list_idx !== -1) {
                closeActiveBlocks(list_idx + 1);
                if (has_blank_line) {
                  list[5] = 0;
                }
              } else {
                closeActiveBlocks(depth);
                pushStack(list);
              }
            } else {
              if (stack.length && stack[stack.length - 1][I_TYPE] === T_LIST_ITEM) {
                list = mkList(l_marker);
                stack[stack.length - 1][I_CONTENT].push(list);
                pushStack(list);
              } else {
                closeActiveBlocks(depth);
                list = mkList(l_marker);
                addBlock(list);
                pushStack(list);
              }
            }

            const content_str = sub_trimmed.slice(l_marker[3]).trim();
            let is_task = 0,
              is_checked = 0,
              task_txt = content_str;
            if (content_str.startsWith("[ ] ")) {
              is_task = 1;
              is_checked = 0;
              task_txt = content_str.slice(4);
            } else if (content_str.startsWith("[x] ") || content_str.startsWith("[X] ")) {
              is_task = 1;
              is_checked = 1;
              task_txt = content_str.slice(4);
            }

            let marker_pos = line_pos;
            while (line[marker_pos] === " " || line[marker_pos] === "\t") {
              ++marker_pos;
            }

            const spaces_after = line.slice(marker_pos + l_marker[3]).match(/^ */)[0].length,
              final_spaces = !content_str || !spaces_after || spaces_after > 4 ? 1 : spaces_after,
              content_indent = rem_indent + l_marker[3] + final_spaces,
              item_node = [
                T_LIST_ITEM,
                ST_NEW,
                [],
                is_task,
                is_checked,
                rem_indent,
                content_indent
              ];
            list[I_CONTENT].push(item_node);
            pushStack(item_node);
            has_blank_line = 0;

            if (task_txt !== "") {
              let child_line = line.slice(marker_pos + l_marker[3]);
              if (is_task) {
                child_line = child_line.replace(/^( *)\[[ xX]\] /, "$1");
              }
              child_line = child_line.slice(final_spaces);
              feedLine(child_line, 1);
            }
            return;
          }
        }
      }

      if (active && active[I_TYPE] === T_P && rem_indent < 4) {
        const aligns = parseTableBorder(sub_trimmed);
        if (aligns) {
          const header_text = active[I_CONTENT],
            headers = splitTableCells(header_text);
          active[I_TYPE] = T_TABLE;
          active[I_STATUS] = ST_NEW;
          active[I_CONTENT] = [headers];
          active[I_ATTR1] = aligns;
          return;
        }
      }

      if (active && active[I_TYPE] === T_TABLE && active[I_STATUS] !== ST_DONE) {
        if (sub_trimmed.indexOf("|") !== -1) {
          const cells = splitTableCells(sub_trimmed);
          active[I_CONTENT].push(cells);
          active[I_STATUS] = ST_UPDATE;
          return;
        } else {
          active[I_STATUS] = ST_DONE;
        }
      }

      let html_type = 0;
      if (line_indent < 4 && sub_trimmed[0] === "<") {
        html_type = parseHtmlBlockStart(sub_trimmed);
      }

      if (active && active[I_TYPE] === T_P && active[I_STATUS] !== ST_DONE) {
        if (html_type >= 1 && html_type <= 6) {
          active[I_STATUS] = ST_DONE;
          addHtmlBlock(sub_line, html_type);
          return;
        }
        active[I_CONTENT] += "\n" + sub_line + (sub_line.endsWith("  ") ? "  " : "");
        active[I_STATUS] = ST_UPDATE;
      } else {
        if (html_type >= 1 && html_type <= 7) {
          addHtmlBlock(sub_line, html_type);
          return;
        }
        addBlock([T_P, ST_NEW, sub_line + (sub_line.endsWith("  ") ? "  " : "")]);
      }
    },
    addBlock = (node) => {
      if (node[I_TYPE] !== T_LIST_ITEM) {
        while (stack.length && stack[stack.length - 1][I_TYPE] === T_LIST) {
          const top = stack.pop();
          if (has_blank_line) {
            markListLoose();
          }
          has_blank_line = 0;
          top[I_STATUS] = ST_DONE;
        }
      }
      if (has_blank_line) {
        markListLoose();
        has_blank_line = 0;
      }
      if (stack.length) {
        const top = stack[stack.length - 1];
        if (!Array.isArray(top[I_CONTENT])) top[I_CONTENT] = [];
        top[I_CONTENT].push(node);
      } else {
        blocks.push(node);
      }
    },
    processBlockInlines = (node, link_defs) => {
      const type = node[I_TYPE],
        status = node[I_STATUS],
        t = status !== ST_DONE ? [...node] : node;

      if (type === T_P || type === T_H) {
        if (typeof t[I_CONTENT] === "string") {
          t[I_CONTENT] = parseInline(
            status !== ST_DONE ? t[I_CONTENT].trimEnd() : t[I_CONTENT],
            link_defs
          );
        }
      } else if (type >= T_BLOCKQUOTE && type <= T_LIST_ITEM) {
        if (Array.isArray(t[I_CONTENT])) {
          if (status !== ST_DONE) {
            t[I_CONTENT] = [...t[I_CONTENT]];
          }
          for (let i = 0; i < t[I_CONTENT].length; ++i) {
            t[I_CONTENT][i] = processBlockInlines(t[I_CONTENT][i], link_defs);
          }
        }
      } else if (type === T_TABLE) {
        const rows = t[I_CONTENT];
        for (let r = 0; r < rows.length; ++r) {
          const row = rows[r];
          for (let c = 0; c < row.length; ++c) {
            if (typeof row[c] === "string") {
              row[c] = parseInline(row[c], link_defs);
            }
          }
        }
      }
      return t;
    },
    cleanCodeBlocks = (nodes) => {
      for (const node of nodes) {
        if (node._cleaned) continue;
        if (node[I_TYPE] === T_CODE_BLOCK && node[I_ATTR1] === null) {
          node[I_CONTENT] = (node[I_CONTENT] || "").replace(/(\n[ \t]*)+$/, "");
          node._cleaned = 1;
        } else if (Array.isArray(node[I_CONTENT])) {
          cleanCodeBlocks(node[I_CONTENT]);
        }
        if (node[I_STATUS] === ST_DONE) {
          node._cleaned = 1;
        }
      }
    },
    prepareResult = (is_eof = 0) => {
      if (is_eof) {
        closeActiveBlocks(0);
        for (const b of blocks) {
          b[I_STATUS] = ST_DONE;
        }
      }
      cleanCodeBlocks(blocks);
      extractLinkDefs(blocks, link_defs);
      const res = [];
      for (const b of blocks) {
        if (b[I_STATUS] === ST_DONE && b._inlineProcessed) {
          res.push(b);
        } else {
          const processed = processBlockInlines(b, link_defs);
          if (processed[I_STATUS] === ST_DONE) {
            processed._inlineProcessed = 1;
          }
          if (processed[I_TYPE] === T_BLOCKQUOTE) {
            detectAlertBlockquote(processed);
          }
          res.push(processed);
        }
      }
      return res;
    },
    detectAlertBlockquote = (b) => {
      const first = b[I_CONTENT][0];
      if (first && first[I_TYPE] === T_P) {
        const p_inlines = first[I_CONTENT],
          first_inline = p_inlines[0];
        if (first_inline && first_inline[I_TYPE] === T_TEXT) {
          const txt = first_inline[I_CONTENT].trim(),
            m = txt.match(/^\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]/i);
          if (m) {
            b[I_ATTR1] = 1;
            b[I_ATTR2] = m[1].toUpperCase();
            if (!(first_inline[I_CONTENT] = txt.slice(m[0].length).trim())) {
              p_inlines.shift();
              if (p_inlines[0] && p_inlines[0][I_TYPE] === T_BR) {
                p_inlines.shift();
              }
            }
          }
        }
      }
    },
    push = (chunk) => {
      raw_text += chunk;
      let nl_idx = raw_text.indexOf("\n");
      while (nl_idx !== -1) {
        const line = raw_text.slice(0, nl_idx);
        raw_text = raw_text.slice(nl_idx + 1);
        feedLine(line);
        nl_idx = raw_text.indexOf("\n");
      }

      if (raw_text !== "") {
        const orig_blocks = blocks,
          orig_stack = stack,
          orig_link_defs = link_defs,
          orig_has_blank_line = has_blank_line,
          temp_blocks = [...blocks],
          temp_stack = [];
        let current_li = temp_blocks;

        while (current_li.length) {
          const last_idx = current_li.length - 1,
            node = current_li[last_idx];
          if (node[I_STATUS] === ST_DONE) {
            break;
          }
          const node_clone = [...node],
            type = node_clone[I_TYPE];
          current_li[last_idx] = node_clone;

          if (type >= T_BLOCKQUOTE && type <= T_LIST_ITEM) {
            temp_stack.push(node_clone);
          }

          if (Array.isArray(node_clone[I_CONTENT])) {
            node_clone[I_CONTENT] = [...node_clone[I_CONTENT]];
            current_li = node_clone[I_CONTENT];
          } else {
            break;
          }
        }

        const temp_link_defs = [...link_defs];

        blocks = temp_blocks;
        stack = temp_stack;
        link_defs = temp_link_defs;
        feedLine(raw_text);
        const res = prepareResult(1);

        blocks = orig_blocks;
        stack = orig_stack;
        link_defs = orig_link_defs;
        has_blank_line = orig_has_blank_line;

        return [res, push];
      } else {
        const res = prepareResult(0);
        return [res, push];
      }
    };

  return push(initial_text || "");
};

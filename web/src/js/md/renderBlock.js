import {
  I_TYPE,
  I_CONTENT,
  I_ATTR1,
  I_ATTR2,
  I_ATTR3,
  T_H,
  T_P,
  T_CODE_BLOCK,
  T_BLOCKQUOTE,
  T_LIST,
  T_LIST_ITEM,
  T_THEMATIC_BREAK,
  T_TABLE,
  T_HTML,
  T_MATH_BLOCK,
  T_TEXT,
  T_STRONG,
  T_EM,
  T_CODE,
  T_LINK,
  T_IMG,
  T_DEL,
  T_BR,
  T_MATH_INLINE
} from "./ast.js";
import htmE from "./htmE.js";

const safeEncodeURI = (str) =>
    str.replace(/%[0-9a-fA-F]{2}|./g, (m) =>
      m.length === 3 || /[\w.\-~!*'();:@&=+$,/?#]/.test(m) ? m : encodeURIComponent(m)
    ),
  safeUrl = (x) => htmE(safeEncodeURI(x || "")),
  titleAttr = (t) => (t ? ' title="' + htmE(t) + '"' : ""),
  // 递归渲染 child block 列表
  renderChildren = (child_li, is_tight) =>
    Array.isArray(child_li) ? child_li.map((c) => renderBlock(c, is_tight)).join("") : "",
  // 渲染 inline 节点列表
  renderInlines = (inline_li) => {
    if (typeof inline_li === "string") return htmE(inline_li);
    if (!Array.isArray(inline_li)) return "";
    let html = "";
    for (const node of inline_li) {
      const type = node[I_TYPE];
      switch (type) {
        case T_TEXT:
          html += htmE(node[I_CONTENT]);
          break;
        case T_STRONG:
        case T_EM:
        case T_DEL:
        case T_CODE: {
          const t =
            type === T_STRONG ? "strong" : type === T_EM ? "em" : type === T_DEL ? "del" : "code";
          html += "<" + t + ">" + renderInlines(node[I_CONTENT]) + "</" + t + ">";
          break;
        }
        case T_LINK:
          html +=
            '<a href="' +
            safeUrl(node[I_ATTR1]) +
            '"' +
            titleAttr(node[I_ATTR2]) +
            ">" +
            renderInlines(node[I_CONTENT]) +
            "</a>";
          break;
        case T_IMG:
          html +=
            '<img src="' +
            safeUrl(node[I_ATTR1]) +
            '" alt="' +
            htmE(node[I_CONTENT] || "") +
            '"' +
            titleAttr(node[I_ATTR2]) +
            ">";
          break;
        case T_BR:
          html += "<br>";
          break;
        case T_HTML:
          html += node[I_CONTENT] || "";
          break;
        case T_MATH_INLINE:
          html += "<c-math>" + node[I_CONTENT] + "</c-math>";
          break;
        default:
          break;
      }
    }
    return html;
  },
  // 渲染表格单元格行
  renderRow = (cell_li, tag, align_li) =>
    "<tr>" +
    cell_li
      .map(
        (cell, i) =>
          "<" +
          tag +
          (align_li[i] ? ' class="' + align_li[i] + '"' : "") +
          ">" +
          renderInlines(cell) +
          "</" +
          tag +
          ">"
      )
      .join("") +
    "</tr>",
  // 递归渲染 block 节点
  renderBlock = (node, is_tight_parent = 0) => {
    const type = node[I_TYPE];
    switch (type) {
      case T_P:
        return is_tight_parent
          ? renderInlines(node[I_CONTENT])
          : "<p>" + renderInlines(node[I_CONTENT]) + "</p>";

      case T_H: {
        const level = node[I_ATTR1];
        return "<h" + level + ">" + renderInlines(node[I_CONTENT]) + "</h" + level + ">";
      }

      case T_CODE_BLOCK: {
        const lang = node[I_ATTR3],
          code = node[I_CONTENT];
        return (
          "<pre><code" +
          (lang ? ' class="language-' + lang + '"' : "") +
          ">" +
          htmE(code || "") +
          "</code></pre>"
        );
      }

      case T_BLOCKQUOTE: {
        const is_alert = node[I_ATTR1],
          alert_type = node[I_ATTR2],
          b_html = renderChildren(node[I_CONTENT], 0);
        return is_alert
          ? '<blockquote class="q ' +
              alert_type.toLowerCase() +
              '"><b class="qt"><i class="qi"></i>' +
              alert_type +
              "</b>" +
              b_html +
              "</blockquote>"
          : "<blockquote>" + b_html + "</blockquote>";
      }

      case T_LIST: {
        const list_type = node[I_ATTR1],
          start_num = node[I_ATTR2],
          tight = node[I_ATTR3],
          l_html = renderChildren(node[I_CONTENT], tight === 1);
        return list_type === 2
          ? "<ol" + (start_num !== 1 ? ' start="' + start_num + '"' : "") + ">" + l_html + "</ol>"
          : "<ul>" + l_html + "</ul>";
      }

      case T_LIST_ITEM: {
        const is_task = node[I_ATTR1],
          is_checked = node[I_ATTR2],
          li_html = renderChildren(node[I_CONTENT], is_tight_parent);
        return is_task
          ? '<li><input type="checkbox" disabled' +
              (is_checked ? " checked" : "") +
              "> " +
              li_html +
              "</li>"
          : "<li>" + li_html + "</li>";
      }

      case T_THEMATIC_BREAK:
        return "<hr>";

      case T_TABLE: {
        const align_li = node[I_ATTR1] || [],
          row_li = node[I_CONTENT] || [];
        if (!row_li.length) return "";
        let html = "<table><thead>" + renderRow(row_li[0], "th", align_li) + "</thead>";
        if (row_li.length > 1) {
          html += "<tbody>";
          for (let r = 1; r < row_li.length; ++r) {
            html += renderRow(row_li[r], "td", align_li);
          }
          html += "</tbody>";
        }
        return html + "</table>";
      }

      case T_HTML:
        return node[I_CONTENT] || "";

      case T_MATH_BLOCK:
        return "<c-math>\n" + node[I_CONTENT] + "\n</c-math>";

      default:
        return "";
    }
  };

export default renderBlock;

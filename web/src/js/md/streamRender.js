import { newEl } from "../dom.js";
import { On } from "../On.js";
import parentScroll from "../parentScroll.js";
import { stream } from "./stream.js";
import renderBlock from "./renderBlock.js";

export default (container, source) => {
  const cursor = newEl("i"),
    scroll_el = parentScroll(container),
    temp_div = newEl("div"),
    html_li = [],
    parse = (html) => {
      temp_div.innerHTML = html;
      return temp_div.firstElementChild || newEl("p");
    };

  container.innerHTML = "";
  cursor.className = "T";
  container.appendChild(cursor);

  let pre_sh = 0,
    pre_st = 0,
    done_cnt = 0,
    raf = null,
    stop = false,
    un_scroll = null,
    auto_scroll = false;

  if (scroll_el) {
    pre_sh = scroll_el.scrollHeight;
    pre_st = scroll_el.scrollTop;

    const onScroll = () => {
      const { scrollTop, scrollHeight, clientHeight } = scroll_el;
      if (auto_scroll) {
        auto_scroll = false;
        pre_st = scrollTop;
        return;
      }

      const last_el = container.lastElementChild || cursor,
        { bottom: scroll_bottom } = scroll_el.getBoundingClientRect(),
        { bottom: cursor_bottom } = last_el.getBoundingClientRect();

      if (scrollTop < pre_st) {
        if (cursor_bottom > scroll_bottom + 24) {
          stop = true;
        }
      } else if (scrollTop > pre_st) {
        if (cursor_bottom <= scroll_bottom + 24 || scrollTop + clientHeight >= scrollHeight - 10) {
          stop = false;
        }
      }
      pre_st = scrollTop;
    };
    un_scroll = On(scroll_el, { scroll: onScroll });
  }

  const render = (block_li, done) => {
      const last_el = container.lastElementChild || cursor,
        scroll_bottom = scroll_el ? scroll_el.getBoundingClientRect().bottom : 0,
        last_bottom = last_el.getBoundingClientRect().bottom,
        was_at_bottom = scroll_el && last_bottom <= scroll_bottom + 24,
        child_li = container.children,
        final_cnt = block_li.length - !done,
        limit = block_li.length + !done;

      for (let i = done_cnt; i < block_li.length; ++i) {
        const block = block_li[i],
          html = renderBlock(block, false),
          child = child_li[i];

        if (!child) {
          container.appendChild(parse(html));
          html_li[i] = html;
        } else if (html_li[i] !== html) {
          container.replaceChild(parse(html), child);
          html_li[i] = html;
        }
      }

      while (child_li.length > limit) {
        container.lastElementChild.remove();
      }
      html_li.length = block_li.length;

      done_cnt = Math.max(0, final_cnt);

      if (!done) {
        container.appendChild(cursor);
      }

      if (was_at_bottom && !stop && scroll_el) {
        if (raf) cancelAnimationFrame(raf);
        raf = requestAnimationFrame(() => {
          const { scrollHeight } = scroll_el;
          if (scrollHeight !== pre_sh) {
            const next_last = container.lastElementChild || cursor,
              { bottom: scroll_bottom } = scroll_el.getBoundingClientRect(),
              { bottom: last_bottom } = next_last.getBoundingClientRect(),
              diff = last_bottom - (scroll_bottom - 24);

            if (diff > 0) {
              auto_scroll = true;
              scroll_el.scrollTop += diff;
            }
            pre_sh = scroll_el.scrollHeight;
          }
        });
      }
    },
    cancelStream = stream(source, (block_li, done, error) => {
      if (error) {
        console.error(error);
      } else {
        render(block_li, done);
      }
    }),
    cancel = () => {
      cancelStream();
      if (raf) cancelAnimationFrame(raf);
      if (un_scroll) un_scroll();
    },
    setStop = (val) => {
      stop = val;
    };

  // 返回 [取消渲染函数, 设置暂停滚动函数]
  return [cancel, setStop];
};

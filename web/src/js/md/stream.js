import parse from "./ast.js";

export const stream = (iterable, onUpdate) => {
  let [block_li, push] = parse(),
    frame_id,
    buf = "",
    aborted;

  queueMicrotask(() => {
    if (!aborted) onUpdate(block_li, false);
  });

  const triggerYield = () => {
    frame_id = 0;
    if (buf) {
      [block_li, push] = push(buf);
      buf = "";
    }
    if (!aborted) {
      onUpdate(block_li, false);
    }
  };

  (async () => {
    try {
      for await (const chunk of iterable) {
        if (aborted) return;
        if ((buf += chunk || "") && !frame_id) {
          frame_id = requestAnimationFrame(triggerYield);
        }
      }
      if (frame_id) cancelAnimationFrame(frame_id);
      if (buf && !aborted) {
        [block_li, push] = push(buf);
      }
      if (!aborted) {
        onUpdate(block_li, true);
      }
    } catch (e) {
      if (!aborted) {
        onUpdate(block_li, true, e);
      }
    }
  })();

  return () => {
    aborted = true;
    if (frame_id) cancelAnimationFrame(frame_id);
    if (iterable.return) iterable.return();
  };
};

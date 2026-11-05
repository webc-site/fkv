import parse from "./ast.js";
import astToHtml from "./lib.js";

// 默认导出：直接将 markdown 文本转换为 HTML 字符串 (兼容普通及测试调用)
export default (text) => {
  const [ast] = parse(text || "");
  for (const b of ast) b[1] = 2;
  return astToHtml(ast);
};

import renderBlock from "./renderBlock.js";

// 渲染顶级 AST 节点列表
export default (ast_li) =>
  Array.isArray(ast_li) ? ast_li.map((b) => renderBlock(b, 0)).join("") : "";

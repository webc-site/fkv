import post from "./post.js";

export default (head, body, css, js) => {
  const content_li = ["<!doctype html><meta charset=UTF-8>"];
  if (head) content_li.push("<head>", head, "</head>");
  content_li.push("<body>");
  if (css) content_li.push("<style>", css, "</style>");
  content_li.push(body);
  if (js) content_li.push('<script type="module">', js, "</script>");
  content_li.push("</body>");

  post("https://codesandbox.io/api/v1/sandboxes/define", {
    "files[index.html][content]": content_li.join("\n")
  });
};

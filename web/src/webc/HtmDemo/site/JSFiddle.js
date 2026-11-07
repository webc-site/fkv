import post from "./post.js";

export default (head, body, css, js) => {
  let html = js ? body + '\n<script type="module">\n' + js + "\n</script>" : body;
  if (head) {
    html = head + "\n" + html;
  }
  post("//jsfiddle.net/api/post/library/pure/", {
    html,
    css,
    js: ""
  });
};

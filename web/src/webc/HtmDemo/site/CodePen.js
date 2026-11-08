import post from "./post.js";

export default (head, body, css, js) => {
  post("//codepen.io/pen/define/", {
    data: JSON.stringify({
      html: body,
      css,
      js,
      head
    })
  });
};

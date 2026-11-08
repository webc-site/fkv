import { newLi } from "../dom.js";

export default () => {
  const [header, h1, b] = newLi("header", "h1", "b");
  header.className = "H1Txt";
  header.append(h1, b);
  return [header, h1, b];
};

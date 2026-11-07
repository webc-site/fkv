import sleep from "@3-/sleep";
import { clearDom } from "../dom.js";

export default async (form) => {
  form.className = "Ok";
  clearDom(form);
  await sleep(600);
};

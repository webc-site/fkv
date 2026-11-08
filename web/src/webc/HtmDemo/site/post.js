import { D, newEl } from "../../../js/dom.js";

export default (action, data) => {
  const form = newEl("form");
  form.method = "POST";
  form.action = action;
  form.target = "_blank";

  for (const [k, v] of Object.entries(data)) {
    const input = newEl("input");
    input.type = "hidden";
    input.name = k;
    input.value = v;
    form.appendChild(input);
  }

  D.body.appendChild(form);
  form.submit();
  D.body.removeChild(form);
};

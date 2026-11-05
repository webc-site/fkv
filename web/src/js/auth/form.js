import { newEl, newLi } from "../dom.js";
import { On } from "../On.js";

const B = "b",
  BUTTON = "button",
  SUBMIT = "submit",
  TEXT = "text",
  PASSWORD = "password",
  ROW = "Row",
  C_MAIN = "Btn Main",
  C_BTNC = "BtnC",
  C_INPUT = "Input",
  INPUT = "input",
  LABEL = "label",
  OFF = "off";

export const newBtn = (className, type = BUTTON) => {
    const el = newEl(BUTTON);
    el.type = type;
    if (className) el.className = className;
    return el;
  },
  newBtnIcon = (p, className = C_BTNC, type = BUTTON) => {
    const [btn, icon] = newLi(BUTTON, B);
    btn.type = type;
    btn.className = className;
    btn.dataset.p = p;
    icon.dataset.p = p;
    btn.append(icon);
    return btn;
  },
  newSubmit = (txt, extra_cls = "") => {
    const btn = newBtn(extra_cls ? C_MAIN + " " + extra_cls : C_MAIN, SUBMIT);
    btn.textContent = txt;
    return btn;
  },
  newRow = (...children) => {
    const row = newEl(B);
    row.className = ROW;
    row.append(...children.filter(Boolean));
    return row;
  },
  makeInput = (type, val, label_txt, attr_map = {}) => {
    const [wrap, input, label] = newLi(B, INPUT, LABEL);
    wrap.className = C_INPUT;
    input.type = type;
    input.placeholder = " ";
    input.required = true;
    if (val) input.value = val;
    for (const k in attr_map) input.setAttribute(k, attr_map[k]);
    label.textContent = label_txt;
    wrap.append(input, label);
    return [wrap, input];
  },
  makeCodeInput = (label_txt) =>
    makeInput(TEXT, "", label_txt, {
      autocomplete: "one-time-code",
      minlength: 6,
      spellcheck: "false"
    }),
  makePwdInput = (label_txt, autocomplete = OFF) =>
    makeInput(PASSWORD, "", label_txt, {
      autocomplete,
      minlength: 6,
      maxlength: 64
    }),
  makeEdit = (val, label_txt, onEdit) => {
    const [wrap, input] = makeInput(TEXT, val, label_txt, {
      readonly: "",
      tabindex: "-1"
    });
    wrap.classList.add("Edit");
    if (onEdit) On(wrap, { click: onEdit });
    return [wrap, input];
  },
  makeBack = (onBack) => {
    const [btn, icon] = newLi("a", "i");
    btn.className = "BtnC back Big";
    icon.className = "Ico";
    btn.append(icon);
    if (onBack) On(btn, { click: onBack });
    return btn;
  },
  makeResend = (cd, onResend) => {
    const el = newEl("c-mail-resend");
    el.setAttribute("spam", "");
    el.setAttribute("cd", cd || 0);
    el.onResend = onResend;
    return el;
  };

export default (el, msg) => {
  el.setCustomValidity(msg);
  el.reportValidity();
  el.addEventListener("input", () => el.setCustomValidity(""), {
    once: true
  });
};

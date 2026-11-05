export default (el, run) => async (e) => {
  if (e && e.preventDefault) e.preventDefault();
  if (el.classList.contains("Ing")) return;
  el.classList.add("Ing");
  try {
    await run(e);
  } finally {
    el.classList.remove("Ing");
  }
};

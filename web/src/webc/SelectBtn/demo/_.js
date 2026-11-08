export default (root) => {
  const el = root.querySelector("c-select-btn");
  if (el) {
    el.addEventListener("submit", (e) => {
      console.log("c-select-btn submit value:", e.value);
    });
  }
};

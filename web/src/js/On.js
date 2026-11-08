export const On = (elem, dict) => {
  const entries = Object.entries(dict);
  entries.forEach(([event, func]) => elem.addEventListener(event, func));
  return () => {
    entries.forEach(([event, func]) => elem.removeEventListener(event, func));
  };
};

export default (group) => {
  const re_li = Object.entries(group).map(([k, v]) => [new RegExp(k), v]);

  return (name) => {
    for (const [re, v] of re_li) {
      if (re.test(name)) {
        return v;
      }
    }
  };
};

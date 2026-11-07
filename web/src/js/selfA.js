// 同站链接返回 url 并阻止默认跳转
export const selfA = (p, e) => {
  if (p.host == location.host) {
    const { hash, pathname, search } = p;
    let url = pathname.slice(1) + search;
    if (hash) {
      url += hash;
    }
    e.preventDefault();
    return url;
  }
};

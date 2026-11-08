const ENT_MAP = {
  amp: "&",
  quot: '"',
  lt: "<",
  gt: ">",
  nbsp: "\u00A0",
  copy: "\u00A9",
  AElig: "\u00C6",
  Dcaron: "\u010E",
  frac34: "\u00BE",
  HilbertSpace: "\u210B",
  DifferentialD: "\u2146",
  ClockwiseContourIntegral: "\u2232",
  ngE: "\u2267\u0338",
  ouml: "\u00F6",
  auml: "\u00E4"
};

export const isPunct = (c) => c > 32 && c < 127 && !/[a-zA-Z\d]/.test(String.fromCharCode(c)),
  htmD = (str, decode_backslashes = 1) =>
    str.replace(/\\([\s\S])|&(#[xX]?[0-9a-fA-F]+|\w+);/g, (m, esc, ent) => {
      if (esc) {
        return decode_backslashes && isPunct(esc.charCodeAt(0)) ? esc : m;
      }
      if (ent[0] === "#") {
        const code = +ent.replace("#", "0");
        return !isNaN(code) && code <= 0x10ffff ? String.fromCodePoint(code || 0xfffd) : m;
      }
      return ENT_MAP[ent] || m;
    });

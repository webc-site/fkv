export default (t) => t.replace(/[&<>"]/g, (c) => "&#" + c.charCodeAt() + ";");

import htm from "./htm.js";
import safe from "./safe.js";

export default (val) => safe(htm(val || ""));

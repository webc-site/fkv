import read from "@3-/read";
import { join } from "node:path";

const pkg_path = join(import.meta.dirname, "../../package.json");
export default JSON.parse(read(pkg_path));

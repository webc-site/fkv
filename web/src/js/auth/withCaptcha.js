import Captcha from "../../webc/Captcha.js";
import { setCaptcha } from "@1-/protoapi";

export default async (run) => {
  try {
    const captcha = await Captcha();
    if (!captcha) return;
    setCaptcha(captcha);
    return await run();
  } catch {}
};

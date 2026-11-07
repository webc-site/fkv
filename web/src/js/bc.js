import { On } from "./On.js";

export const bc = (name) => {
  const channel = new BroadcastChannel(name),
    // oxlint-disable-next-line unicorn/require-post-message-target-origin
    post = (data) => channel.postMessage(data),
    on = (func) => On(channel, { message: (e) => func(e.data) });
  return [post, on];
};

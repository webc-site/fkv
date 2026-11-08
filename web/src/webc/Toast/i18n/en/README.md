# Prompt pop-up window at the edge of the page

- **Function call**: Provides normal and error prompt interfaces
- **Auto stack**: Multiple tips automatically calculate the spacing and stack vertically
- **Scheduled and manual shutdown**: Supports scheduled and manual shutdown

## Use the demo

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Prompt content";
});

toastErr((el) => {
  el.textContent = "Error content";
});

toast(
  (el) => {
    el.textContent = "Automatically close after 99 seconds";
  },
  99
);
```

## Interface parameters

### toast(render, timeout)

Create and display prompts.

- `render`: `(el) => void`, rendering callback, `el` is the prompt box element.
- `timeout`: `Number`, timeout seconds. Default 9. Set to 0 to not automatically shut down.
- Return value: prompt box element. Supports `el.close()` off.

### toastErr(render, timeout)

Create and display error messages. The parameters are the same as `toast`, with `.ERR` style class.

## Style Class

- `.Toast`: Prompt box base class.
- `.ERR`: Error status.
- `.animated` / `.fadeInR` / `.fadeOutR`: animated transition.
- `.x`: Close button.
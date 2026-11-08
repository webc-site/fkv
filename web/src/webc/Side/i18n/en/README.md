# Drawer panels that slide out from the side

- Automatically create internal scrolling structures
- Click the outer area to automatically collapse
- Swipe left and right gesture to close
- Side sliding positioning in left and right directions
- Provide open and close events

## Use the demo

```html
<button class="open-btn">Open</button>

<c-side class="right">
  <div>Drawer contents</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## Interface

### Method

- `open(el)`: Export function, open drawer
- `close(el)`: Export function, close drawer
- `el.open()`: instance method, open drawer
- `el.close()`: instance method, close the drawer

### Event

- `open`: Fires when turned on
- `close`: Fires when closed

## Style Class

- `.right`: right slide out positioning
- `.top`: Slide out positioning
- `.bottom`: slide out of positioning
- (If not set, the default is to slide out left)
- `.open`: open status
- `.H`: hidden state
- `.T`: animation transition state
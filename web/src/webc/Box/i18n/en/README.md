# Create and display popup layer

- Functional call to create and display popup layer
- Automatically remove elements from the DOM when closed
- Prevent Esc key cancellation behavior

## Use the demo

```html
<button>Click to pop up</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Title";
  description.textContent = "Text";
  button.className = "Btn Main";
  button.textContent = "OK";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Interface parameters

### Default exported functions

`Box()`

- **Return value**: `HTMLDialogElement`, the dialog element of `Box` style class has been added.

## Style Class

### `.Box`

Applies to the `dialog` element to set positioning, centering, and background mask blur.

### `.Lg`

A style class applied to the inner content, setting the background color, rounded corners, shadow and vertical layout, and defining the layout of the inner `h3` and `p` .
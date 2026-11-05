# Safely parse and render Markdown text to HTML.

- **Security Sanitization**: Defend against XSS attacks using native `setHTML` API or `DOMParser` cleaners
- **Content Extraction**: Automatically read the text content of the default slot during initialization and render it
- **Built-in typesetting**: preset list, table, code block, quote and warning box styles

## Use the demo

### Static Rendering

```html
<c-md>
# Title
This is a piece of Markdown content
</c-md>
```

### Dynamic Assignment

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#New title\nNew content";
</script>
```

## Interface parameters

Pass in Markdown text directly within the tag (as the default slot), or update dynamically via `value`.

## Style description

The `.Md` style class is automatically added when the component is initialized. Supports the following Markdown extended warning box layout (triggered by reference syntax):

- `[!NOTE]`：Prompt information
- `[!TIP]`: Suggestions/Tips
- `[!IMPORTANT]`: Important attention
- `[!WARNING]`: Risk warning
- `[!CAUTION]`: Extremely high risk warning

## Security Mechanism

When rendering, call the native `setHTML` API first to safely inject HTML. If the browser does not support it, it will automatically downgrade to a recursive cleaner based on `DOMParser` and implement the following security policy:
- Remove non-safe tags (e.g. `script`, `iframe`, etc.)
- Clear `on*` event attributes
- Defend against XSS attacks by filtering attributes starting with dangerous protocols such as `javascript:`, `vbscript:`, and `data:` (except `data:image/`).
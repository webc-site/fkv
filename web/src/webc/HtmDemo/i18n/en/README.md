# Debugging HTML and JS code online

- Render a debugging bar below the code and provide CodePen and JSFiddle debugging portals
- Automatically remember the selected debugging platform
- Supports injecting callbacks through the `.gen` attribute to dynamically generate HTML, CSS and JS code
- Automatically identify child elements with `language-js` class name and run in JS mode

## Use the demo

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demo</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Return [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Interface parameters

### Properties and methods

- `.gen`: Injected callback function. **This callback must be injected for the debug bar to be displayed. **
  - parameter:
    - `type`: Injected content type (`HTML = 1`, `JS = 2`).
    - `text`: The code text extracted from the child element.
  - Return value: `[htm, css, js]`
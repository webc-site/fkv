# Render and display component demos, documentation, and installation instructions

- Extract and parse the title, introduction and content of the README document
- Sandbox to run interactive demos of components and isolate styles
- Convert code blocks in documents into interactive presentations
- Adaptive layout, wide screen column display, narrow screen tab switching display

## Use the demo

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Initialize and obtain the operation interface
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Interface parameters

### `WebCDemo(el)`

Initialize the component.

- **parameter**
  - `el`:`HTMLElement`, component container.
- **Return value**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: Set up the demo module.
  - `setMd(name, md, lang)`: Set the component name, README document content and current language.
  - `unMount()`: Unload the component and clean up the event listener.

### Demo module `mod` attribute

- `CSS`:`string`, the style sheet path of the component.
- `HTM`:`string`, the component's HTML template.
- `default`:`function(root)`, component initialization function, receives shadow root container.

## Style Class

### `.M`

content area.

### `.L`

The left panel contains instructions and documentation.

### `.R`

Right panel, containing the demo.

### `c-nav`

Narrow screen navigation bar.
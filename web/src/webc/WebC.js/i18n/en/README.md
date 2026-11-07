# Documentation and source code for rendering components or tool functions

- Two-column responsive layout, with documents displayed on the left and source code displayed on the right
- Documentation and source code support Edit and Code shortcut buttons to jump to the source address.
- The title and introduction are displayed at the top, supporting i18n automatic translation.

## Use the demo

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Property parameters

- `src`: URL address of the source code.
- `href`: URL address of document markdown, supports using `{code}` placeholder to adapt to the current language.
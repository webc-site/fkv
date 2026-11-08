# Navigation menu adapted to different screen sizes

- Desktop: Links are displayed in horizontal tiles
- Mobile version: Collapse into menu button, click to slide out the sidebar drawer
- Route activation: automatically compare the current path and hash value, and add activation status class name to the matching link `A`
- Automatically close: When the mobile terminal clicks on a link or the external background of the sidebar, the sidebar automatically closes.

## Use

```html
<c-nav-menu>
  <a href="/">Front Page</a>
  <a href="/doc">Document</a>
  <a href="/about">About</a>
</c-nav-menu>
```

## Readme

When the component is initialized, if there is no `c-side` in the child element, the mobile menu button and `c-side` sidebar will be automatically created, and all `a` tags will be moved into the sidebar.

## Style Class

- **A**: The activated link will automatically add the class name `A`
# Routing portal hosting component library documentation and demo site

- **Full screen layout**: Absolute positioning fills the viewport, with built-in scrolling and navigation.
- **Dynamic Routing**: Parse URL paths and dynamically load component demos or homepages.
- **Multi-language**: Respond and update the navigation bar and page copy in real time.
- **Status switching**: Automatically maintain the style classes of loading and homepage.

## Use the demo

```html
<c-webc-site></c-webc-site>
```

## Style Class

The component will switch the following class names within itself:

- `Ing`: Route loading or language pack parsing.
- `A`: Currently on the home page.
# Automatically switch navigation menu and return button

- Automatically wrap child elements to generate navigation menu and return button structures
- Automatically trace routing history and calculate the previous return path
- Display the navigation menu when the route matches the menu item, otherwise display the back button

## Use the demo

```html
<c-nav-l>
  <a href="/">Front Page</a>
  <a href="/doc">Document</a>
</c-nav-l>
```

## Style Class

- `B`: activates the return button state. Show the back button and hide the navigation menu. Automatically switched within the component based on routing.
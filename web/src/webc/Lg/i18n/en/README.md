# Frosted glass background and edge highlight effect

Semi-transparent border and inner shadow create micro-relief edge highlights.
As the basic style dependency of button and input box components.

## Use the demo

```html
<main class="Lg">
  <article>
    <h3>Title</h3>
    <pre>Content text</pre>
  </article>
</main>
```

## Style Class

### `.Lg`
Card basic style. Apply `backdrop-filter` blur, `border` micro-embossed border, and `box-shadow` inner shadow. Apply `svg/glass.svg#h` filter on hover.

### `main.Lg`
The page's main block container. Rounded corners `24px` with responsive padding (`16px` if width is less than `800px`, `24px` otherwise).

### `.Btn`, `.BtnC`, `.Input`
Basic component class name. After introducing this style, you will automatically obtain flexible layout, hierarchical relationships and basic properties of frosted glass.
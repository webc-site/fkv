# Top navigation bar that automatically collapses or expands when the page scrolls

- **Auto Hide**: Automatically hide when scrolling down the page.
- **Auto-expand**: Automatically displayed when the page is scrolled up or the scroll bar disappears.
- **Performance Optimization**: Use `requestAnimationFrame` to throttle scrolling.
- **Release resources**: Event listeners and observers are automatically removed when the component is destroyed.

## Use the demo

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">Link</a>
      </c-nav>
    </nav>
    <div class="content">Swipe down to hide the navigation, swipe up to show it</div>
  </c-vs>
</main>
<style>
  main {
    position: relative;
    height: 300px;
    border: 1px solid #000;
  }
  nav.Nav > c-nav {
    background: #fffc;
    backdrop-filter: blur(4px);
    border-bottom: 1px solid #ccc;
  }
  .content {
    height: 200dvh;
    background: linear-gradient(to bottom, #fff, #ccc);
    padding: 80px 20px 20px;
    color: #666;
    text-align: center;
  }
</style>
```

## Instructions for use

Must be used as a child or descendant element of `<c-vs>`.

## Style Status

The component switches the following class name on `nav.Nav`:

- `D`: Show status (slide down or at top).
- `I`: Transition state being hidden.
- `H`: Hidden status.

## Style Variables

- `--top`: Automatically updated by the component to a negative value of height, used for positioning and transition.
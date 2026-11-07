# Virtual Scroll Bar

- **Two-way scrolling**: Provides vertical (`c-vs`) and horizontal (`c-hs`) scrolling.
- **Size response**: Monitor content and area size changes, automatically update the scroll bar size and position, and automatically hide when there is no overflow.
- **Interactive support**: Supports wheel scrolling, slider dragging and track click positioning.
- **Status Hide**: Show and widen the slider when hovering or scrolling, and automatically hide it when stationary.

## Vertical Scroll

```html
<div style="height:300px;border:1px solid #000">
  <c-vs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-vs>
</div>
<style>b { display: flex; border: 2px solid #090; padding: 30px; margin: 30px; }</style>
```

## Horizontal Scroll

```html
<div style="width:300px;border:1px solid #000">
  <c-hs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-hs>
</div>
<style>b { display: flex; border: 2px solid #090; margin: 30px; padding: 30px; }</style>
```

## Style Part

Customize the style of Shadow DOM internal elements via `::part()`:

- `::part(scroll)`: Scroll the content area.
- `::part(bar)`: Scroll bar track.
- `::part(si)`: Scroll bar slider.
- `::part(i)`: Internal wrapping layer.
- `::part(drag)`: The scroll bar track in the dragging state.

## Style Class

- `.drag`: Applied to the `body` element, indicating that it is in a drag-scrolling state.

## Custom properties

- `--si-bg`: Slider background color.
- `--si-anim`: Slider animation.
- `--cursorScrollhSvg`: The mouse cursor when the horizontal scroll bar is hovered.
- `--cursorScrollvSvg`: The mouse cursor when the vertical scroll bar is hovered.
# Floating Bubble Prompt

Display hover tooltip information when the mouse is hovered or the element is activated.

- Based on native Web Component, lightweight and high-performance
- Supports adaptive spatial positioning in four directions (class="top/bottom/left/right")
- It uses a black frosted glass texture gradient background and white characters, with exquisite physical edge contour highlights.
- Built-in position adaptive calculation, automatically adjusts the pop-up direction according to the remaining space in the viewport
- Use `slot="pop"` to display floating prompt content

## Use the demo

```html
<!-- Automatically calculate direction (recommended, no need to declare class) -->
<c-pophover>
  <button>Automatic</button>
  <div slot="pop">This is a bubble tip</div>
</c-pophover>

<!-- Fixed Direction -->
<c-pophover class="left">
  <button>Left</button>
  <div slot="pop">This is a bubble tip</div>
</c-pophover>

<c-pophover class="right">
  <button>Right</button>
  <div slot="pop">This is a bubble tip</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Down</button>
  <div slot="pop">This is a bubble tip</div>
</c-pophover>

<c-pophover class="top">
  <button>Superior</button>
  <div slot="pop">This is a bubble tip</div>
</c-pophover>
```

## CSS variables

Custom styles are supported via the following CSS variables:

| Variable Name | Default Value | Readme |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | The space between the bubble tooltip and the triggering element |
| `--pophover-margin` | `24px` | Viewport edge safety margin during adaptive positioning calculation |
| `--pophover-bg-top` | `#18181ce0` | Bubble gradient background top color |
| `--pophover-bg-mid` | `#121215e5` | The background color of the arrows on the left and right sides of the bubble |
| `--pophover-bg-bottom` | `#0c0c0feb` | The color of the bottom and upper and lower side arrows of the bubble gradient background |

## Icon tooltip style (`.Ico`)

For pure icon type floating prompt buttons (such as "Edit", "Delete", etc.), you can apply a unified icon and hover style by adding the `.Ico` class name to the outer container or component itself.

### Structure example

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">Edit</div>
  </c-pophover>
</b>
```

The basic layout, size (18x18px), and hover color changes have been unified in `.Ico > c-pophover > a`. Developers only need to specify the corresponding icon file separately, such as:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```